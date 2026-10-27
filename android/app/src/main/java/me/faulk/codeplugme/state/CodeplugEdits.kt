package me.faulk.codeplugme.state

import kotlinx.serialization.json.JsonElement
import me.faulk.codeplugme.model.ChannelEdit
import me.faulk.codeplugme.model.ContactEdit
import me.faulk.codeplugme.model.DumpChannel
import me.faulk.codeplugme.model.DumpContact
import me.faulk.codeplugme.model.DumpGroupList
import me.faulk.codeplugme.model.DumpQytBand
import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.DumpQytSettings
import me.faulk.codeplugme.model.DumpRadioId
import me.faulk.codeplugme.model.DumpScanList
import me.faulk.codeplugme.model.DumpZone
import me.faulk.codeplugme.model.EditSpec
import me.faulk.codeplugme.model.GroupListEdit
import me.faulk.codeplugme.model.MoveOp
import me.faulk.codeplugme.model.QytBandEdit
import me.faulk.codeplugme.model.QytChannelEdit
import me.faulk.codeplugme.model.QytEditSpec
import me.faulk.codeplugme.model.QytSettingsEdit
import me.faulk.codeplugme.model.RadioIdEdit
import me.faulk.codeplugme.model.ScanListEdit
import me.faulk.codeplugme.model.ZoneEdit

/**
 * Turns an edited record into the smallest edit batch that expresses the change.
 *
 * Minimal batches are not an optimisation. The core writes only the fields a
 * batch names, and a codeplug is full of bytes this app does not model, so
 * sending a whole record back would mean re-encoding fields nobody touched and
 * risking the ones nobody understands. Diffing against the current record and
 * sending the difference is what keeps an unmodelled byte untouched.
 *
 * Every function here returns null when nothing changed, so the caller can skip
 * a round trip and, more importantly, not push an empty entry onto the undo
 * stack.
 */

/** The minimal AnyTone channel edit from [old] to [new], or null if identical. */
fun channelEdit(old: DumpChannel, new: DumpChannel): ChannelEdit? {
    if (old == new) return null
    return ChannelEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        rxFrequencyHz = new.rxFrequencyHz.takeIf { it != old.rxFrequencyHz },
        txFrequencyHz = new.txFrequencyHz.takeIf { it != old.txFrequencyHz },
        mode = new.mode.takeIf { it != old.mode }?.let(::editValue),
        power = new.power.takeIf { it != old.power }?.let(::editValue),
        bandwidth = new.bandwidth.takeIf { it != old.bandwidth }?.let(::editValue),
        colorCode = new.colorCode.takeIf { it != old.colorCode },
        timeSlot = new.timeSlot.takeIf { it != old.timeSlot },
        contactIndex = new.contactIndex.takeIf { it != old.contactIndex },
        radioIdIndex = new.radioIdIndex.takeIf { it != old.radioIdIndex },
        groupListIndex = new.groupListIndex.takeIf { it != old.groupListIndex },
        rxSignalingMode = new.rxSignalingMode.takeIf { it != old.rxSignalingMode }
            ?.let(::snakeEditValue),
        txSignalingMode = new.txSignalingMode.takeIf { it != old.txSignalingMode }
            ?.let(::snakeEditValue),
        rxCtcss = new.rxCtcss.takeIf { it != old.rxCtcss },
        txCtcss = new.txCtcss.takeIf { it != old.txCtcss },
        rxDcs = new.rxDcs.takeIf { it != old.rxDcs },
        txDcs = new.txDcs.takeIf { it != old.txDcs },
        squelchMode = new.squelchMode.takeIf { it != old.squelchMode }?.let(::snakeEditValue),
        optionalSignaling = new.optionalSignaling.takeIf { it != old.optionalSignaling }
            ?.let(::snakeEditValue),
        admit = new.admit.takeIf { it != old.admit }?.let(::snakeEditValue),
        scanListIndex = new.scanListIndex.takeIf { it != old.scanListIndex },
        dtmfIdIndex = new.dtmfIdIndex.takeIf { it != old.dtmfIdIndex },
        twoToneIdIndex = new.twoToneIdIndex.takeIf { it != old.twoToneIdIndex },
        fiveToneIdIndex = new.fiveToneIdIndex.takeIf { it != old.fiveToneIdIndex },
        twoToneDecodeIndex = new.twoToneDecodeIndex.takeIf { it != old.twoToneDecodeIndex },
        rxOnly = new.rxOnly.takeIf { it != old.rxOnly },
        talkAround = new.talkAround.takeIf { it != old.talkAround },
        callConfirm = new.callConfirm.takeIf { it != old.callConfirm },
        workAlone = new.workAlone.takeIf { it != old.workAlone },
        simplexTdma = new.simplexTdma.takeIf { it != old.simplexTdma },
        rxAprs = new.rxAprs.takeIf { it != old.rxAprs },
    )
}

/** The minimal zone edit, or null if identical. Zones send `members`, not `channels`. */
fun zoneEdit(old: DumpZone, new: DumpZone): ZoneEdit? {
    if (old == new) return null
    return ZoneEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        members = new.channels.takeIf { it != old.channels },
    )
}

/** The minimal scan-list edit, or null if identical. */
fun scanListEdit(old: DumpScanList, new: DumpScanList): ScanListEdit? {
    if (old == new) return null
    return ScanListEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        members = new.members.takeIf { it != old.members },
        priorityChannelSelect = new.priorityChannelSelect
            .takeIf { it != old.priorityChannelSelect },
        priorityChannel1 = new.priorityChannel1.takeIf { it != old.priorityChannel1 },
        priorityChannel2 = new.priorityChannel2.takeIf { it != old.priorityChannel2 },
        lookBackA = new.lookBackA.takeIf { it != old.lookBackA },
        lookBackB = new.lookBackB.takeIf { it != old.lookBackB },
        dropoutDelay = new.dropoutDelay.takeIf { it != old.dropoutDelay },
        dwellTime = new.dwellTime.takeIf { it != old.dwellTime },
        revertChannel = new.revertChannel.takeIf { it != old.revertChannel },
    )
}

/** The minimal contact edit, or null if identical. */
fun contactEdit(old: DumpContact, new: DumpContact): ContactEdit? {
    if (old == new) return null
    return ContactEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        number = new.number.takeIf { it != old.number },
        callType = new.callType.takeIf { it != old.callType }?.let(::snakeEditValue),
    )
}

/** The minimal group-list edit, or null if identical. */
fun groupListEdit(old: DumpGroupList, new: DumpGroupList): GroupListEdit? {
    if (old == new) return null
    return GroupListEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        members = new.members.takeIf { it != old.members },
    )
}

/** The minimal radio-ID edit, or null if identical. */
fun radioIdEdit(old: DumpRadioId, new: DumpRadioId): RadioIdEdit? {
    if (old == new) return null
    return RadioIdEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        number = new.number.takeIf { it != old.number },
    )
}

/**
 * The minimal QYT channel edit, or null if identical.
 *
 * Only the fields the core has setters for appear: name, both frequencies, both
 * tones, and scan-skip. The other flag bits are decoded for display but are not
 * writable, because only this one was confirmed against real hardware. Offering
 * a control for the rest would silently discard the user's change on the next
 * reload, which is worse than not offering it.
 *
 * The TX frequency is the one field with three states. Absent means "leave it",
 * a number means "set it", and an explicit JSON null means "clear it, making the
 * channel receive-only".
 */
fun qytChannelEdit(old: DumpQytChannel, new: DumpQytChannel): QytChannelEdit? {
    if (old == new) return null
    val tx: JsonElement? = when {
        new.txFrequencyHz == old.txFrequencyHz -> null
        new.txFrequencyHz == null -> QytChannelEdit.CLEAR_TX_FREQUENCY
        else -> QytChannelEdit.txFrequencyHzValue(new.txFrequencyHz!!)
    }
    return QytChannelEdit(
        index = new.index,
        name = new.name.takeIf { it != old.name },
        rxFrequencyHz = new.rxFrequencyHz.takeIf { it != old.rxFrequencyHz },
        txFrequencyHz = tx,
        rxTone = new.rxTone.takeIf { it != old.rxTone },
        txTone = new.txTone.takeIf { it != old.txTone },
        scanAdd = new.scanAdd.takeIf { it != old.scanAdd },
    )
}

/** The minimal QYT settings edit, or null if nothing changed. */
fun qytSettingsEdit(old: DumpQytSettings, new: DumpQytSettings): QytSettingsEdit? {
    if (old == new) return null
    val edit = QytSettingsEdit(
        tmr = new.tmr.takeIf { it != old.tmr },
        squelch = new.squelch.takeIf { it != old.squelch },
        tot = new.tot.takeIf { it != old.tot },
        volume = new.volume.takeIf { it != old.volume },
        backlightTimer = new.backlightTimer.takeIf { it != old.backlightTimer },
        beep = new.beep.takeIf { it != old.beep },
        scanRev = new.scanRev.takeIf { it != old.scanRev }?.let(::snakeEditValue),
        rememberPower = new.rememberPower.takeIf { it != old.rememberPower },
    )
    return edit.takeUnless { it.isEmpty }
}

/**
 * The minimal VFO edit, or null if identical.
 *
 * The repeater offset and shift direction are deliberately absent: the core has
 * no setters for them because their encoding is assumed rather than confirmed
 * against the radio.
 */
fun qytBandEdit(old: DumpQytBand, new: DumpQytBand): QytBandEdit? {
    if (old == new) return null
    return QytBandEdit(
        index = new.index,
        frequencyHz = new.frequencyHz.takeIf { it != old.frequencyHz },
        rxTone = new.rxTone.takeIf { it != old.rxTone },
        txTone = new.txTone.takeIf { it != old.txTone },
        bandwidth = new.bandwidth.takeIf { it != old.bandwidth }?.let(::editValue),
        power = new.power.takeIf { it != old.power }?.let(::editValue),
        stepHz = new.stepHz.takeIf { it != old.stepHz },
    )
}

/**
 * Which AnyTone channel fields a bulk edit should set, and to what.
 *
 * Every field is opt-in: null means "leave this alone on every selected
 * channel". Mirrors the macOS bulk sheet's per-field checkboxes, and the eight
 * fields here are the ones that make sense to apply across a selection.
 */
data class BulkChannelUpdate(
    val mode: String? = null,
    val power: String? = null,
    val bandwidth: String? = null,
    val colorCode: Int? = null,
    val timeSlot: Int? = null,
    val contactIndex: Long? = null,
    val groupListIndex: Int? = null,
    val radioIdIndex: Int? = null,
) {
    /** True when nothing was opted in, so the caller can skip the whole batch. */
    val isEmpty: Boolean get() = this == BulkChannelUpdate()

    /** Build the batch that applies this to every slot in [slots]. */
    fun toSpec(slots: Collection<Int>): EditSpec = EditSpec(
        channels = slots.map { index ->
            ChannelEdit(
                index = index,
                mode = mode?.let(::editValue),
                power = power?.let(::editValue),
                bandwidth = bandwidth?.let(::editValue),
                colorCode = colorCode,
                timeSlot = timeSlot,
                contactIndex = contactIndex,
                groupListIndex = groupListIndex,
                radioIdIndex = radioIdIndex,
            )
        }
    )
}

/**
 * Which QYT channel fields a bulk edit should set.
 *
 * Scan-skip only, because it is the only shared flag the core can write. See
 * [qytChannelEdit].
 */
data class BulkQytChannelUpdate(val scanAdd: Boolean? = null) {
    /** True when nothing was opted in. */
    val isEmpty: Boolean get() = scanAdd == null

    /** Build the batch that applies this to every slot in [slots]. */
    fun toSpec(slots: Collection<Int>): QytEditSpec = QytEditSpec(
        channels = slots.map { QytChannelEdit(index = it, scanAdd = scanAdd) }
    )
}

/** A move batch for one AnyTone family, chosen by [section]. */
fun moveSpec(section: CodeplugSection, from: Int, to: Int): EditSpec {
    val op = listOf(MoveOp(from, to))
    return when (section) {
        CodeplugSection.CHANNELS -> EditSpec(moveChannels = op)
        CodeplugSection.ZONES -> EditSpec(moveZones = op)
        CodeplugSection.SCAN_LISTS -> EditSpec(moveScanLists = op)
        CodeplugSection.CONTACTS -> EditSpec(moveContacts = op)
        CodeplugSection.GROUP_LISTS -> EditSpec(moveGroupLists = op)
        CodeplugSection.RADIO_IDS -> EditSpec(moveRadioIds = op)
        else -> EditSpec()
    }
}

/** A removal batch for one AnyTone family, chosen by [section]. */
fun removeSpec(section: CodeplugSection, slots: Collection<Int>): EditSpec {
    val s = slots.toList()
    return when (section) {
        CodeplugSection.CHANNELS -> EditSpec(removeChannels = s)
        CodeplugSection.ZONES -> EditSpec(removeZones = s)
        CodeplugSection.SCAN_LISTS -> EditSpec(removeScanLists = s)
        CodeplugSection.CONTACTS -> EditSpec(removeContacts = s)
        CodeplugSection.GROUP_LISTS -> EditSpec(removeGroupLists = s)
        CodeplugSection.RADIO_IDS -> EditSpec(removeRadioIds = s)
        else -> EditSpec()
    }
}
