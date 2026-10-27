import Foundation

/// Errors surfaced from the Rust core across the C FFI boundary.
enum RadioCoreError: LocalizedError {
    case native(String)

    var errorDescription: String? {
        switch self {
        case .native(let message): return message
        }
    }
}

/// Thin Swift wrapper over the codeplugme-core C FFI (`core/src/ffi.rs`,
/// `core/include/codeplugme_core.h`). All calls are synchronous and blocking;
/// run device operations off the main thread.
enum RadioCore {

    /// Box carried through the C progress callback's `user` pointer.
    private final class ProgressBox {
        let handler: (Int, Int) -> Void
        init(_ handler: @escaping (Int, Int) -> Void) { self.handler = handler }
    }

    /// C trampoline: forwards (done, total) to the boxed Swift closure.
    private static let progressTrampoline: codeplugme_progress_cb = { done, total, user in
        guard let user = user else { return }
        Unmanaged<ProgressBox>.fromOpaque(user).takeUnretainedValue().handler(Int(done), Int(total))
    }

    /// Take ownership of a Rust-allocated C string, copy it, and free it.
    private static func consume(_ ptr: UnsafeMutablePointer<CChar>?) -> String? {
        guard let ptr = ptr else { return nil }
        defer { codeplugme_string_free(ptr) }
        return String(cString: ptr)
    }

    /// Build the thrown error from an `err_out` pointer, freeing it.
    private static func nativeError(_ err: UnsafeMutablePointer<CChar>?) -> RadioCoreError {
        .native(consume(err) ?? "unknown native error")
    }

    /// Decode JSON produced by the core into a Codable model.
    private static func decode<T: Decodable>(_ type: T.Type, from json: String) throws -> T {
        do {
            return try JSONDecoder().decode(type, from: Data(json.utf8))
        } catch {
            throw RadioCoreError.native("failed to decode core JSON: \(error)")
        }
    }

    /// The exact byte length this build expects a full codeplug .bin to be. A
    /// file of any other length is from a different radio model or firmware
    /// version and can't be parsed or written; callers use this to reject an
    /// incompatible file with a clear message before touching it.
    static var expectedCodeplugSize: Int { codeplugme_codeplug_size() }

    /// Every radio this build supports, as the core describes them. Read once
    /// per call; the list is a handful of entries and never changes at runtime.
    static var supportedModels: [SupportedModel] {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let json = consume(codeplugme_models_json(&err)) else {
            _ = consume(err)
            return []
        }
        return (try? decode([SupportedModel].self, from: json)) ?? []
    }

    /// Which radio the image file at `binPath` belongs to. Throws when the file
    /// can't be read or matches no supported radio.
    static func detectModel(binPath: String) throws -> RadioModel {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let id = consume(codeplugme_detect_model(binPath, &err)) else { throw nativeError(err) }
        guard let model = RadioModel(rawValue: id) else {
            throw RadioCoreError.native("the core reported an unknown radio model “\(id)”")
        }
        return model
    }

    /// The exact byte length of a full codeplug image for `model`.
    static func codeplugSize(for model: RadioModel) throws -> Int {
        var err: UnsafeMutablePointer<CChar>? = nil
        let size = codeplugme_codeplug_size_for(model.rawValue, &err)
        guard size != 0 else { throw nativeError(err) }
        return size
    }

    /// Enumerate serial ports, flagging likely radios.
    static func listPorts() throws -> [PortEntry] {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let json = consume(codeplugme_ports_json(&err)) else { throw nativeError(err) }
        return try decode([PortEntry].self, from: json)
    }

    /// Identify the radio on `port`; returns its model/version string.
    static func identify(port: String) throws -> String {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let model = consume(codeplugme_identify(port, &err)) else { throw nativeError(err) }
        return model
    }

    /// Identify the radio on `port`, reporting which model answered as well as
    /// its raw identity string. `model` selects which protocol to speak; nil
    /// lets the core infer it from the port's USB identity.
    static func identity(port: String, model: RadioModel? = nil) throws -> RadioIdentity {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let json = consume(codeplugme_identify_json(port, model?.rawValue, &err)) else {
            throw nativeError(err)
        }
        return try decode(RadioIdentity.self, from: json)
    }

    /// Read the full codeplug from the radio into the file at `path`.
    /// `model` selects which protocol to speak; nil lets the core infer it from
    /// the port's USB identity. `progress` is called on the calling
    /// (background) thread.
    static func backup(port: String, model: RadioModel? = nil, to path: String,
                       progress: @escaping (Int, Int) -> Void) throws {
        var err: UnsafeMutablePointer<CChar>? = nil
        let box = ProgressBox(progress)
        let user = Unmanaged.passRetained(box).toOpaque()
        defer { Unmanaged<ProgressBox>.fromOpaque(user).release() }
        if codeplugme_backup(port, model?.rawValue, path, progressTrampoline, user, &err) != 0 {
            throw nativeError(err)
        }
    }

    /// Write the codeplug file at `path` back to the radio. `force` must be
    /// true or the core refuses (same gate as the CLI). The core takes the model
    /// from the file itself and refuses to write if the connected radio doesn't
    /// identify as that model, and read-back verifies every block.
    static func restore(port: String, from path: String, force: Bool,
                        progress: @escaping (Int, Int) -> Void) throws {
        var err: UnsafeMutablePointer<CChar>? = nil
        let box = ProgressBox(progress)
        let user = Unmanaged.passRetained(box).toOpaque()
        defer { Unmanaged<ProgressBox>.fromOpaque(user).release() }
        if codeplugme_restore(port, path, force, progressTrampoline, user, &err) != 0 {
            throw nativeError(err)
        }
    }

    /// Parse a codeplug .bin offline into channels and zones.
    static func dump(binPath: String) throws -> CodeplugDump {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let json = consume(codeplugme_dump_json(binPath, &err)) else { throw nativeError(err) }
        return try decode(CodeplugDump.self, from: json)
    }

    /// Apply a batch of edits to `input`, writing the result to `output` (the
    /// paths may be equal). Edits round-trip through the Rust codeplug model and
    /// are verified by re-parsing.
    ///
    /// `spec` is generic over the encodable edit shapes because each radio has
    /// its own: the AnyTone still sends its `EditSpec` bare, which the core
    /// accepts for backward compatibility, while the QYT sends a
    /// `QytEditEnvelope` that names the model it is addressed to.
    static func applyEdits<Spec: Encodable>(input: String, spec: Spec, output: String) throws {
        let data: Data
        do {
            data = try JSONEncoder().encode(spec)
        } catch {
            throw RadioCoreError.native("failed to encode edits JSON: \(error)")
        }
        let json = String(decoding: data, as: UTF8.self)
        var err: UnsafeMutablePointer<CChar>? = nil
        if codeplugme_apply_edits(input, json, output, &err) != 0 {
            throw nativeError(err)
        }
    }

    // MARK: - Repeater import

    /// Encode a model to the JSON string the core expects.
    private static func encodeJSON<T: Encodable>(_ value: T, what: String) throws -> String {
        do {
            return String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
        } catch {
            throw RadioCoreError.native("failed to encode \(what) JSON: \(error)")
        }
    }

    /// Where the core caches the free repeater databases between fetches:
    /// `~/Library/Caches/CodeplugMe/geo`, created on first use.
    static func geoCacheDirectory() throws -> URL {
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask,
                                                 appropriateFor: nil, create: true)
        let dir = caches.appendingPathComponent("CodeplugMe/geo", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Download repeaters near `query` for the codeplug at `binURL`, nearest
    /// first, each flagged `duplicate` when that codeplug already has it. Blocks
    /// on the network; call it off the main thread.
    static func geoFetch(query: GeoQuery, binURL: URL, cacheDir: URL) throws -> [Repeater] {
        let json = try encodeJSON(query, what: "query")
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let out = consume(codeplugme_geo_fetch_json(json, binURL.path, cacheDir.path, &err)) else {
            throw nativeError(err)
        }
        return try decode([Repeater].self, from: out)
    }

    /// How many repeaters may be picked for the codeplug at `binURL` in `mode`.
    static func geoCapacity(binURL: URL, mode: BuildMode) throws -> GeoCapacity {
        var err: UnsafeMutablePointer<CChar>? = nil
        guard let out = consume(codeplugme_geo_capacity_json(binURL.path, mode.rawValue, &err)) else {
            throw nativeError(err)
        }
        return try decode(GeoCapacity.self, from: out)
    }

    /// Write the codeplug at `binURL` plus exactly `repeaters` to `outURL`.
    /// `binURL` is only read. The core refuses, rather than truncates, a
    /// selection that doesn't fit.
    static func geoBuild(binURL: URL, repeaters: [Repeater], options: GeoBuildOptions,
                         outURL: URL) throws {
        let repeatersJSON = try encodeJSON(repeaters, what: "repeaters")
        let optionsJSON = try encodeJSON(options, what: "build options")
        var err: UnsafeMutablePointer<CChar>? = nil
        if codeplugme_geo_build(binURL.path, repeatersJSON, optionsJSON, outURL.path, &err) != 0 {
            throw nativeError(err)
        }
    }
}
