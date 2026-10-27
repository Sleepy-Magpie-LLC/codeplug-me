package me.faulk.codeplugme.state

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
import me.faulk.codeplugme.model.QytChannelEdit
import me.faulk.codeplugme.model.QytEditSpec
import me.faulk.codeplugme.model.RadioIdEdit
import me.faulk.codeplugme.model.ScanListEdit
import me.faulk.codeplugme.model.ZoneEdit

/**
 * Every way the UI can change a codeplug.
 *
 * These are extensions rather than methods so [CodeplugStore] stays focused on
 * the staging machinery (work file, undo, recovery) and this file holds the
 * per-family plumbing. All of them funnel into `stage` or `stageQyt`, so undo,
 * unsaved marks, recovery and save behave identically no matter which family
 * was touched.
 *
 * Records are always addressed by their slot index and looked up by it, never
 * by list position, so a sorted or filtered list cannot commit to the wrong
 * record.
 */

// -------------------------------------------------------------- AnyTone edits

/** Apply an edited channel, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpChannel) {
    val old = state.value.channels.firstOrNull { it.index == new.index } ?: return
    val edit = channelEdit(old, new) ?: return
    if (stage(EditSpec(channels = listOf(edit)), "Updated channel ${new.index + 1}")) {
        markUnsaved(CodeplugSection.CHANNELS, listOf(new.index))
    }
}

/** Apply an edited zone, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpZone) {
    val old = state.value.zones.firstOrNull { it.index == new.index } ?: return
    val edit = zoneEdit(old, new) ?: return
    if (stage(EditSpec(zones = listOf(edit)), "Updated zone ${new.index + 1}")) {
        markUnsaved(CodeplugSection.ZONES, listOf(new.index))
    }
}

/** Apply an edited scan list, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpScanList) {
    val old = state.value.scanLists.firstOrNull { it.index == new.index } ?: return
    val edit = scanListEdit(old, new) ?: return
    if (stage(EditSpec(scanLists = listOf(edit)), "Updated scan list ${new.index + 1}")) {
        markUnsaved(CodeplugSection.SCAN_LISTS, listOf(new.index))
    }
}

/** Apply an edited contact, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpContact) {
    val old = state.value.contacts.firstOrNull { it.index == new.index } ?: return
    val edit = contactEdit(old, new) ?: return
    if (stage(EditSpec(contacts = listOf(edit)), "Updated contact ${new.index + 1}")) {
        markUnsaved(CodeplugSection.CONTACTS, listOf(new.index))
    }
}

/** Apply an edited group list, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpGroupList) {
    val old = state.value.groupLists.firstOrNull { it.index == new.index } ?: return
    val edit = groupListEdit(old, new) ?: return
    if (stage(EditSpec(groupLists = listOf(edit)), "Updated group list ${new.index + 1}")) {
        markUnsaved(CodeplugSection.GROUP_LISTS, listOf(new.index))
    }
}

/** Apply an edited radio ID, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpRadioId) {
    val old = state.value.radioIds.firstOrNull { it.index == new.index } ?: return
    val edit = radioIdEdit(old, new) ?: return
    if (stage(EditSpec(radioIds = listOf(edit)), "Updated radio ID ${new.index + 1}")) {
        markUnsaved(CodeplugSection.RADIO_IDS, listOf(new.index))
    }
}

// ------------------------------------------------------------------ QYT edits

/** Apply an edited QYT channel, if anything actually changed. */
suspend fun CodeplugStore.update(new: DumpQytChannel) {
    val old = state.value.qytChannels.firstOrNull { it.index == new.index } ?: return
    val edit = qytChannelEdit(old, new) ?: return
    if (stageQyt(QytEditSpec(channels = listOf(edit)), "Updated channel ${new.index + 1}")) {
        markUnsaved(CodeplugSection.QYT_CHANNELS, listOf(new.index))
    }
}

/**
 * Apply edited QYT radio settings.
 *
 * Applied as one batch on an explicit Apply rather than per keystroke, which is
 * why the settings screen keeps a local draft: a live-bound field would stage a
 * core round trip and an undo entry for every character typed.
 *
 * The LCD static message rides along because it shares that screen, and a user
 * changing both expects one Apply rather than two undo entries.
 */
suspend fun CodeplugStore.updateQytSettings(new: DumpQytSettings, staticMessage: String) {
    val old = state.value.qytSettings ?: return
    val edit = qytSettingsEdit(old, new)
    val message = staticMessage.takeIf { it != state.value.qytStaticMessage }
    if (edit == null && message == null) return
    val noun = if (edit == null) "Updated display message" else "Updated radio settings"
    if (stageQyt(QytEditSpec(settings = edit, staticMessage = message), noun)) {
        markUnsaved(CodeplugSection.QYT_SETTINGS, listOf(0))
    }
}

/** Apply edited QYT VFO channels, as one batch. */
suspend fun CodeplugStore.updateQytBands(new: List<DumpQytBand>) {
    val current = state.value.qytBands
    val edits = new.mapNotNull { band ->
        current.firstOrNull { it.index == band.index }?.let { qytBandEdit(it, band) }
    }
    if (edits.isEmpty()) return
    val noun = if (edits.size == 1) "VFO channel" else "VFO channels"
    if (stageQyt(QytEditSpec(bands = edits), "Updated $noun")) {
        markUnsaved(CodeplugSection.QYT_BANDS, edits.map { it.index })
    }
}

// ----------------------------------------------------------------- bulk edits

/** Apply [update] to every selected AnyTone channel slot. */
suspend fun CodeplugStore.bulkUpdateChannels(slots: Collection<Int>, update: BulkChannelUpdate) {
    if (update.isEmpty || slots.isEmpty()) return
    val noun = if (slots.size == 1) "channel" else "${slots.size} channels"
    if (stage(update.toSpec(slots), "Updated $noun")) {
        markUnsaved(CodeplugSection.CHANNELS, slots.toList())
    }
}

/** Apply [update] to every selected QYT channel slot. */
suspend fun CodeplugStore.bulkUpdateQytChannels(
    slots: Collection<Int>,
    update: BulkQytChannelUpdate,
) {
    if (update.isEmpty || slots.isEmpty()) return
    val noun = if (slots.size == 1) "channel" else "${slots.size} channels"
    if (stageQyt(update.toSpec(slots), "Updated $noun")) {
        markUnsaved(CodeplugSection.QYT_CHANNELS, slots.toList())
    }
}

// ----------------------------------------------------------------------- adds

/**
 * Add an empty record to the first free slot and return its index.
 *
 * The core chooses the slot and does not report it back, so it is recovered by
 * diffing the slot set across the reload. Returning it lets the caller open the
 * new record's editor immediately, which is what makes "add" feel like one
 * action rather than two.
 */
suspend fun CodeplugStore.addChannel(): Int? =
    stageAdd(CodeplugSection.CHANNELS, "channel", { it.channels.map(DumpChannel::index) }) {
        stage(EditSpec(addChannels = listOf(ChannelEdit(name = "New channel"))), "Added channel")
    }

/** Add an empty zone and return its slot. */
suspend fun CodeplugStore.addZone(): Int? =
    stageAdd(CodeplugSection.ZONES, "zone", { it.zones.map(DumpZone::index) }) {
        stage(EditSpec(addZones = listOf(ZoneEdit(name = "New zone"))), "Added zone")
    }

/** Add an empty scan list and return its slot. */
suspend fun CodeplugStore.addScanList(): Int? =
    stageAdd(CodeplugSection.SCAN_LISTS, "scan list", { it.scanLists.map(DumpScanList::index) }) {
        stage(EditSpec(addScanLists = listOf(ScanListEdit(name = "New scan list"))), "Added scan list")
    }

/** Add an empty contact and return its slot. */
suspend fun CodeplugStore.addContact(): Int? =
    stageAdd(CodeplugSection.CONTACTS, "contact", { it.contacts.map(DumpContact::index) }) {
        stage(EditSpec(addContacts = listOf(ContactEdit(name = "New contact"))), "Added contact")
    }

/** Add an empty group list and return its slot. */
suspend fun CodeplugStore.addGroupList(): Int? =
    stageAdd(CodeplugSection.GROUP_LISTS, "group list", { it.groupLists.map(DumpGroupList::index) }) {
        stage(EditSpec(addGroupLists = listOf(GroupListEdit(name = "New group list"))), "Added group list")
    }

/** Add an empty radio ID and return its slot. */
suspend fun CodeplugStore.addRadioId(): Int? =
    stageAdd(CodeplugSection.RADIO_IDS, "radio ID", { it.radioIds.map(DumpRadioId::index) }) {
        stage(EditSpec(addRadioIds = listOf(RadioIdEdit(name = "New radio ID"))), "Added radio ID")
    }

/** Add an empty QYT channel and return its slot. */
suspend fun CodeplugStore.addQytChannel(): Int? =
    stageAdd(CodeplugSection.QYT_CHANNELS, "channel", { it.qytChannels.map(DumpQytChannel::index) }) {
        stageQyt(QytEditSpec(addChannels = listOf(QytChannelEdit(name = "NEW"))), "Added channel")
    }

// ------------------------------------------------------------ removes / moves

/** Remove records from [section]. Works for one slot or a whole selection. */
suspend fun CodeplugStore.remove(section: CodeplugSection, slots: Collection<Int>) {
    if (slots.isEmpty()) return
    val noun = if (slots.size == 1) "1 record" else "${slots.size} records"
    val ok = if (section == CodeplugSection.QYT_CHANNELS) {
        stageQyt(QytEditSpec(removeChannels = slots.toList()), "Removed $noun")
    } else {
        stage(removeSpec(section, slots), "Removed $noun")
    }
    if (ok) slots.forEach { markRemoved(section, it) }
}

/**
 * Relocate a record to a free slot.
 *
 * The core rejects an occupied or out-of-range target, which surfaces as an
 * error and leaves the work file untouched.
 */
suspend fun CodeplugStore.move(section: CodeplugSection, from: Int, to: Int) {
    val ok = if (section == CodeplugSection.QYT_CHANNELS) {
        stageQyt(
            QytEditSpec(moveChannels = listOf(me.faulk.codeplugme.model.MoveOp(from, to))),
            "Moved to slot ${to + 1}",
        )
    } else {
        stage(moveSpec(section, from, to), "Moved to slot ${to + 1}")
    }
    if (ok) markMoved(section, from, to)
}
