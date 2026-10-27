import Foundation

/// The user saying "the radio on this port is that model", for a port whose USB
/// identity names no radio.
///
/// It is tied to a port rather than kept as a bare model because it must not
/// leak: picking "QYT KT-5000" for one adapter says nothing about the next one
/// plugged in, and carrying it over would be exactly the silent misattribution
/// the core refuses to make on its own.
struct ManualModelSelection: Equatable {
    let port: String
    let model: RadioModel
}

/// Which radio we believe is on `port`: its USB attribution when the core could
/// make one, otherwise the user's explicit pick for that same port. Nil means
/// nobody has said, and nothing may be read or written until somebody does.
func effectiveModel(of port: PortEntry, manual: ManualModelSelection?) -> RadioModel? {
    if let attributed = port.model { return attributed }
    guard let manual = manual, manual.port == port.name else { return nil }
    return manual.model
}

/// Why `port` must not receive a document belonging to `documentModel`, phrased
/// for the user, or nil when the write may go ahead.
///
/// This is the layer in front of the core's guard, not a replacement for it.
/// `write_verified_image` still handshakes the radio and refuses on a mismatch
/// before emitting a byte; this exists so the common cases surface as a sentence
/// the user can act on instead of a failed transfer.
func writeRefusalReason(port: PortEntry, documentModel: RadioModel?,
                        manual: ManualModelSelection?) -> String? {
    guard let documentModel = documentModel else {
        return "No codeplug is open, so there is nothing to write."
    }
    guard let target = effectiveModel(of: port, manual: manual) else {
        return "The adapter on \(port.name) doesn’t identify which radio is behind it. "
            + "Choose \(documentModel.displayName) for it on the Device tab, then try again."
    }
    guard target == documentModel else {
        return "This codeplug is for a \(documentModel.displayName), but \(port.name) is "
            + "a \(target.displayName). Writing it would overwrite the wrong radio."
    }
    return nil
}

/// The ports a document belonging to `documentModel` may be written to.
///
/// Everything else is withheld from the Write-to-Radio menu rather than offered
/// and then rejected: a destructive action the app already knows will fail
/// should not be one click away from a confirmation dialog.
func writableTargets(among ports: [PortEntry], for documentModel: RadioModel?,
                     manual: ManualModelSelection?) -> [PortEntry] {
    ports.filter {
        writeRefusalReason(port: $0, documentModel: documentModel, manual: manual) == nil
    }
}
