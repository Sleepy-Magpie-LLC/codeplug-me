import SwiftUI

/// The QYT KT-5000's two VFO channels (A and B).
///
/// # The encoding these records hide
///
/// A VFO's frequency is not the packed BCD the channel table uses: it stores
/// one decimal digit per byte. The core decodes and encodes it
/// (`core/src/qyt/codeplug/bands.rs`), and this pane only ever moves the decoded
/// hertz value across the FFI. Reading those bytes as BCD gives a
/// plausible-looking wrong frequency, which is exactly the kind of bug that gets
/// a radio transmitting somewhere it should not, so none of that codec is
/// repeated here.
///
/// The image still holds two unused WP-9900-shaped slots (C and D) after B; the
/// core byte-preserves them and never exposes them here.
///
/// The repeater offset and shift direction are read-only: the core has no
/// setters for them because their encoding is assumed rather than confirmed
/// against the radio.
struct QytBandsPane: View {
    @EnvironmentObject private var store: CodeplugStore

    /// Pane-local draft of both VFOs. Applied as one batch, which is one core
    /// round trip and one undo entry per intent rather than per keystroke.
    @State private var draft: [DumpQytBand] = []

    /// Tuning steps the radio offers, in hertz. The core validates against its
    /// own list and rejects anything else with a readable error, so a stale
    /// entry here fails loudly instead of writing a bad step byte.
    private static let steps: [UInt32] = [2_500, 5_000, 6_250, 10_000, 12_500, 25_000]

    var body: some View {
        Group {
            if store.qytBands.isEmpty {
                noVfosState
            } else {
                form
            }
        }
        .onAppear { reseed() }
        // Re-seed from the document when it changes underneath the pane, so an
        // undo of an applied change is reflected here.
        .onChange(of: store.qytBands) { draft = $0 }
    }

    private var form: some View {
        Form {
            ForEach($draft) { $vfo in
                Section(header: Text(header(vfo))) {
                    TextField("Frequency (MHz)", value: frequencyMHz($vfo), format: mhzFormat)
                    QytToneField(label: "RX tone", tone: $vfo.rxTone)
                    QytToneField(label: "TX tone", tone: $vfo.txTone)
                    Picker("Bandwidth", selection: $vfo.bandwidth) {
                        ForEach(store.capabilities.bandwidthOptions, id: \.tag) {
                            Text($0.label).tag($0.tag)
                        }
                    }
                    Picker("Power", selection: $vfo.power) {
                        ForEach(store.capabilities.powerOptions, id: \.tag) {
                            Text($0.label).tag($0.tag)
                        }
                    }
                    stepPicker($vfo)
                    LabeledContent("Repeater offset", value: offsetLabel(vfo))
                    LabeledContent("Shift", value: vfo.shift)
                }
            }

            Section {
                Text("The repeater offset and shift direction are read-only: their byte "
                    + "encoding is assumed from another model and has not been confirmed "
                    + "against this radio, so writing them could put the transmit "
                    + "frequency somewhere unintended.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .safeAreaInset(edge: .bottom) { ApplyRevertFooter(hasChanges: hasChanges,
                                                          onRevert: reseed,
                                                          onApply: apply) }
    }

    private var noVfosState: some View {
        VStack(spacing: Spacing.inline) {
            Text("No VFO channels")
                .font(.title3.weight(.medium))
            Text("This codeplug has no QYT VFO records.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// The tuning-step picker. An unlisted or missing step selects as "Unknown",
    /// which sends nothing, so opening the pane never rewrites a step byte the
    /// core could not decode.
    private func stepPicker(_ vfo: Binding<DumpQytBand>) -> some View {
        Picker("Tuning step", selection: vfo.stepHz) {
            if !Self.steps.contains(where: { $0 == vfo.wrappedValue.stepHz }) {
                Text("Unknown").tag(UInt32?.none)
            }
            ForEach(Self.steps, id: \.self) {
                Text(stepLabel($0)).tag(UInt32?.some($0))
            }
        }
    }

    /// "VFO A", with an unprogrammed slot saying so rather than showing a bare
    /// 0 MHz with no explanation.
    private func header(_ vfo: DumpQytBand) -> String {
        let name = "VFO \(qytBandName(vfo.index))"
        return vfo.programmed ? name : "\(name) (unprogrammed)"
    }

    /// The VFO frequency in MHz. The core owns the digit-per-byte codec; this
    /// only scales the hertz value it reports.
    private func frequencyMHz(_ vfo: Binding<DumpQytBand>) -> Binding<Double> {
        Binding(get: { Double(vfo.wrappedValue.frequencyHz ?? 0) / 1_000_000 },
                set: { vfo.wrappedValue.frequencyHz = hzFromMHz($0) })
    }

    private func offsetLabel(_ vfo: DumpQytBand) -> String {
        guard let hz = vfo.offsetHz else { return "—" }
        return "\(formatMHz(hz)) MHz"
    }

    private func stepLabel(_ hz: UInt32) -> String {
        let khz = Double(hz) / 1000
        return khz == khz.rounded() ? "\(Int(khz)) kHz" : String(format: "%.2f kHz", khz)
    }

    /// True when any VFO's draft holds something the document doesn't.
    private var hasChanges: Bool { draft != store.qytBands }

    private func reseed() { draft = store.qytBands }

    private func apply() { store.updateQytBands(draft) }
}
