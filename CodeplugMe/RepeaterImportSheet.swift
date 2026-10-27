import AppKit
import CoreLocation
import SwiftUI
import UniformTypeIdentifiers

/// "Add Repeaters from Database…": find repeaters near a place, pick some, and
/// add them to the open codeplug as a staged, undoable edit (the default), or
/// write the codeplug plus the picks to a new .bin, which then opens.
///
/// The search and results live in the store's `RepeaterImportModel`, so they
/// survive closing the sheet for the rest of the app session.
struct RepeaterImportSheet: View {
    @EnvironmentObject private var store: CodeplugStore
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var model: RepeaterImportModel
    @AppStorage(repeaterBookContactKey) private var repeaterBookContact = ""

    @State private var repeaterBookToken: String?

    @State private var isFetching = false
    @State private var fetchError: String?
    @State private var capacityError: String?
    @State private var buildError: String?
    @State private var showingOverLimit = false
    @State private var showingSourceSettings = false

    private static let kmPerMile = 1.609344

    private var supportsDMR: Bool { store.capabilities.supportsDMR }

    /// RepeaterBook needs both a token and a contact email before it can be asked.
    private var repeaterBookAuth: RepeaterBookAuth? {
        let contact = repeaterBookContact.trimmingCharacters(in: .whitespaces)
        guard let token = repeaterBookToken, !token.isEmpty, !contact.isEmpty else { return nil }
        return RepeaterBookAuth(token: token, contact: contact)
    }

    /// The databases to ask. RadioID is DMR-only, so it is never sent for a
    /// radio without DMR.
    private var sources: [RepeaterSource] {
        var out: [RepeaterSource] = []
        if model.useHearham { out.append(.hearham) }
        if model.useRadioID && supportsDMR { out.append(.radioid) }
        if model.useRepeaterBook && repeaterBookAuth != nil { out.append(.repeaterbook) }
        return out
    }

    private var hasLocation: Bool {
        switch model.locationInput {
        case .place: return !model.place.trimmingCharacters(in: .whitespaces).isEmpty
        case .coordinates: return Double(model.latText) != nil && Double(model.lonText) != nil
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.stack) {
            Text("Add Repeaters from Database")
                .font(.headline)
            searchForm
            fetchRow
            Divider()
            resultsHeader
            resultsTable
            counter
            Divider()
            buttons
        }
        .padding(Spacing.section)
        .frame(width: 960)
        .frame(minHeight: min(620, EditorSheet<EmptyView>.maxSheetHeight),
               maxHeight: EditorSheet<EmptyView>.maxSheetHeight)
        .onAppear {
            repeaterBookToken = RepeaterBookKeychain.token()
            // Off when the credentials have gone missing since last time;
            // otherwise the user's choice from the last visit stands.
            if repeaterBookAuth == nil { model.useRepeaterBook = false }
            model.discardResults(unlessFor: store.fileURL)
            refreshCapacity()
        }
        .onChange(of: model.mode) { _ in refreshCapacity() }
        .sheet(isPresented: $showingSourceSettings, onDismiss: reloadRepeaterBook) {
            SourceSettingsSheet()
        }
        .onChange(of: model.isOverLimit) { over in
            if over { showingOverLimit = true }
        }
        .alert("Too many channels", isPresented: $showingOverLimit) {
            Button("OK", role: .cancel) {}
        } message: {
            Text("You selected \(model.selectedCount) channels but only \(model.maxSelectable) fit. "
                 + "Deselect \(model.excess).")
        }
    }

    /// Re-read the RepeaterBook credentials after the settings sheet closes, and
    /// turn the source on when they have just become complete.
    private func reloadRepeaterBook() {
        repeaterBookToken = RepeaterBookKeychain.token()
        if repeaterBookAuth != nil { model.useRepeaterBook = true }
    }

    // MARK: - Search

    /// Location, radius, sources, build mode and zone name.
    private var searchForm: some View {
        Grid(alignment: .leading, horizontalSpacing: Spacing.inline, verticalSpacing: Spacing.inline) {
            GridRow {
                label("Location")
                HStack(spacing: Spacing.inline) {
                    Picker("Location", selection: $model.locationInput) {
                        ForEach(LocationInput.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .fixedSize()
                    switch model.locationInput {
                    case .place:
                        TextField("City, address or ZIP", text: $model.place)
                            .textFieldStyle(.roundedBorder)
                    case .coordinates:
                        TextField("Latitude", text: $model.latText)
                            .textFieldStyle(.roundedBorder)
                        TextField("Longitude", text: $model.lonText)
                            .textFieldStyle(.roundedBorder)
                    }
                }
            }
            GridRow {
                label("Radius")
                HStack(spacing: Spacing.inline) {
                    Slider(value: $model.radiusMiles, in: 5...150, step: 5)
                        .frame(width: 280)
                    Text("\(Int(model.radiusMiles)) mi")
                        .monospacedDigit()
                }
            }
            GridRow {
                label("Sources")
                HStack(spacing: Spacing.section) {
                    Toggle("Hearham", isOn: $model.useHearham)
                    if supportsDMR {
                        Toggle("RadioID (DMR)", isOn: $model.useRadioID)
                    }
                    Toggle("RepeaterBook", isOn: $model.useRepeaterBook)
                        .disabled(repeaterBookAuth == nil)
                    Button(repeaterBookAuth == nil ? "Set Up RepeaterBook…" : "RepeaterBook Settings…") {
                        showingSourceSettings = true
                    }
                    if repeaterBookAuth == nil {
                        Text(supportsDMR ? "Needs your own token. Hearham and RadioID need no setup."
                             : "Needs your own token. Hearham needs no setup.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            GridRow {
                label("Build")
                HStack(spacing: Spacing.inline) {
                    Picker("Build", selection: $model.mode) {
                        ForEach(BuildMode.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .fixedSize()
                    Text(model.mode == .clean
                         ? "Keeps settings and contacts, replaces all channels, zones and scan lists."
                         : "Adds the picks to the existing channels.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            GridRow {
                label("Zone name")
                TextField("Nearby", text: $model.zoneName)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 200)
            }
        }
    }

    /// A right-aligned form label for the search grid.
    private func label(_ text: String) -> some View {
        Text(text)
            .gridColumnAlignment(.trailing)
    }

    /// The Fetch button, its progress, and any error.
    private var fetchRow: some View {
        HStack(spacing: Spacing.inline) {
            Button("Fetch", action: fetch)
                .disabled(isFetching || !hasLocation || sources.isEmpty)
            if isFetching {
                ProgressView()
                    .controlSize(.small)
                Text("Downloading repeater lists…")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let fetchError {
                Text(fetchError)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(2)
            }
        }
    }

    // MARK: - Results

    /// Select All / None and the band and mode filters.
    private var resultsHeader: some View {
        HStack(spacing: Spacing.inline) {
            Button("Select All") { model.selectAll() }
            Button("Select None") { model.selectNone() }
            Spacer()
            Picker("Band", selection: $model.bandFilter) {
                ForEach(BandFilter.allCases) { Text($0.label).tag($0) }
            }
            .labelsHidden()
            .fixedSize()
            Picker("Mode", selection: $model.modeFilter) {
                ForEach(ModeFilter.allCases) { Text($0.label).tag($0) }
            }
            .labelsHidden()
            .fixedSize()
        }
        .disabled(model.rows.isEmpty)
    }

    /// The fetched repeaters with a checkbox each.
    private var resultsTable: some View {
        Table(model.visibleRows) {
            TableColumn("") { row in
                Toggle("", isOn: Binding(
                    get: { model.isSelected(row) },
                    set: { model.setSelected(row, $0) }))
                    .labelsHidden()
                    .disabled(!model.isSelectable(row))
            }
            .width(24)
            TableColumn("Distance") { row in
                Text(row.repeater.distanceKm.map { String(format: "%.1f mi", $0 / Self.kmPerMile) } ?? "?")
                    .monospacedDigit()
            }
            .width(70)
            TableColumn("Callsign") { row in
                HStack(spacing: Spacing.tight) {
                    Text(row.repeater.callsign)
                    if row.repeater.duplicate && model.mode == .merge {
                        Text("already in codeplug")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .width(min: 90, ideal: 180)
            TableColumn("RX MHz") { row in
                Text(formatMHz(row.repeater.rxHz)).monospacedDigit()
            }
            .width(85)
            TableColumn("TX MHz") { row in
                Text(formatMHz(row.repeater.txHz)).monospacedDigit()
            }
            .width(85)
            TableColumn("Tone / CC") { row in Text(row.repeater.toneLabel) }
                .width(75)
            TableColumn("Mode") { row in Text(row.repeater.mode.label) }
                .width(55)
            TableColumn("Location") { row in Text(row.repeater.location) }
            TableColumn("Source") { row in Text(row.repeater.source.label) }
                .width(90)
        }
        .frame(minHeight: 220)
        .overlay {
            if model.hasFetched && model.rows.isEmpty && !isFetching {
                Text("No repeaters found within \(Int(model.radiusMiles)) mi.")
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// "N selected / M available", red when over, plus the capacity notes.
    private var counter: some View {
        VStack(alignment: .leading, spacing: Spacing.tight) {
            HStack(spacing: Spacing.inline) {
                Text("\(model.selectedCount) selected / \(model.maxSelectable) available")
                    .monospacedDigit()
                    .foregroundStyle(model.isOverLimit ? .red : .primary)
                if model.isOverLimit {
                    Text("Deselect \(model.excess) to build.")
                        .foregroundStyle(.red)
                }
                Spacer()
                if model.hasRepeaterBookResults {
                    Text("Data courtesy of RepeaterBook.com")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if let reason = model.capacity?.blockedReason {
                Text(reason)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            if let limit = model.capacity?.scanListLimit {
                Text("The scan list holds the nearest \(limit) picks.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let capacityError {
                Text(capacityError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
    }

    /// Close, Save as New File and the default Add to Codeplug, with any build
    /// error beside them.
    private var buttons: some View {
        HStack(spacing: Spacing.stack) {
            if let buildError {
                Text(buildError)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(2)
            } else {
                Text(model.mode == .clean
                     ? "Replaces the open codeplug's channels. Undo or Revert to Saved brings them back."
                     : "Adds to the open codeplug as an unsaved change you can undo.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button("Close", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button("Save as New File…", action: saveAsNewFile)
                .disabled(!model.canBuild || isFetching)
                .help("Write the open codeplug plus the picks to a new .bin instead")
            Button("Add to Codeplug", action: addToCodeplug)
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .disabled(!model.canBuild || isFetching)
        }
    }

    // MARK: - Actions

    /// Ask the core how many picks fit in the current build mode. Local and fast,
    /// so it runs inline.
    private func refreshCapacity() {
        do {
            model.capacity = try RadioCore.geoCapacity(binURL: Recovery.workURL, mode: model.mode)
            capacityError = nil
        } catch {
            model.capacity = nil
            capacityError = error.localizedDescription
        }
    }

    /// Resolve the location, then download and rank repeaters off the main
    /// thread.
    private func fetch() {
        isFetching = true
        fetchError = nil
        buildError = nil
        let radiusKm = model.radiusMiles * Self.kmPerMile
        let sources = sources
        let auth = sources.contains(.repeaterbook) ? repeaterBookAuth : nil
        let binURL = Recovery.workURL
        let binFile = store.fileURL
        Task { @MainActor in
            defer { isFetching = false }
            do {
                let center = try await resolveLocation()
                let query = GeoQuery(lat: center.lat, lon: center.lon, radiusKm: radiusKm,
                                     country: center.country, state: center.state,
                                     sources: sources, repeaterbook: auth)
                let found = try await Task.detached {
                    try RadioCore.geoFetch(query: query, binURL: binURL,
                                           cacheDir: try RadioCore.geoCacheDirectory())
                }.value
                model.setResults(found, for: binFile)
                            } catch {
                fetchError = error.localizedDescription
            }
        }
    }

    /// The search center and the country and state the state-searched sources
    /// need.
    private struct Center {
        let lat: Double
        let lon: Double
        let country: String?
        let state: String?
    }

    /// Turn the typed place or coordinates into a `Center`. Forward geocoding
    /// needs no location permission. A failed reverse lookup for typed
    /// coordinates is not fatal: Hearham searches by coordinates alone.
    private func resolveLocation() async throws -> Center {
        let geocoder = CLGeocoder()
        switch model.locationInput {
        case .place:
            let query = model.place.trimmingCharacters(in: .whitespaces)
            let marks: [CLPlacemark]
            do {
                marks = try await geocoder.geocodeAddressString(query)
            } catch {
                throw RadioCoreError.native("Could not find “\(query)”: \(error.localizedDescription)")
            }
            guard let mark = marks.first, let location = mark.location else {
                throw RadioCoreError.native("Could not find “\(query)”.")
            }
            return center(location.coordinate.latitude, location.coordinate.longitude, mark)
        case .coordinates:
            guard let lat = Double(model.latText), let lon = Double(model.lonText),
                  (-90...90).contains(lat), (-180...180).contains(lon) else {
                throw RadioCoreError.native("Latitude must be -90 to 90 and longitude -180 to 180.")
            }
            let mark = try? await geocoder.reverseGeocodeLocation(CLLocation(latitude: lat, longitude: lon)).first
            return center(lat, lon, mark)
        }
    }

    /// Build a `Center`, expanding a US state abbreviation to its full name.
    private func center(_ lat: Double, _ lon: Double, _ mark: CLPlacemark?) -> Center {
        let area = mark?.administrativeArea
        let state = mark?.isoCountryCode == "US" ? USStates.fullName(area) : area
        return Center(lat: lat, lon: lon, country: mark?.country, state: state)
    }

    /// The build options from the form.
    private var buildOptions: GeoBuildOptions {
        let name = model.zoneName.trimmingCharacters(in: .whitespaces)
        return GeoBuildOptions(mode: model.mode, zoneName: name.isEmpty ? "Nearby" : name)
    }

    /// The default: stage the picks into the open codeplug, then close. The
    /// picks are flagged as already in the codeplug for the next visit.
    private func addToCodeplug() {
        guard model.canBuild else { return }
        buildError = nil
        guard store.addRepeaters(model.selectedRepeaters, options: buildOptions) else {
            // The store reports the failure in its own error alert too; keep the
            // reason beside the buttons where the user is looking.
            buildError = store.errorMessage
            store.errorMessage = nil
            return
        }
        model.markSelectionAdded()
        refreshCapacity()
        dismiss()
    }

    /// The alternative: ask for a new .bin, write the codeplug plus the picks
    /// to it, and open it. The open document is never written.
    private func saveAsNewFile() {
        guard model.canBuild, let base = store.fileURL else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "bin") ?? .data]
        panel.nameFieldStringValue = base.deletingPathExtension().lastPathComponent + "-repeaters.bin"
        panel.title = "Save New Codeplug"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if url.standardizedFileURL == base.standardizedFileURL {
            buildError = "Choose a new file. Save as New File never overwrites the open codeplug."
            return
        }
        do {
            try RadioCore.geoBuild(binURL: Recovery.workURL, repeaters: model.selectedRepeaters,
                                   options: buildOptions, outURL: url)
        } catch {
            buildError = error.localizedDescription
            return
        }
        store.open(url: url)
        dismiss()
    }
}
