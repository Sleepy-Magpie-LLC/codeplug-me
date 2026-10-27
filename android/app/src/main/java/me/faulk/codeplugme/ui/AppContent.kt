package me.faulk.codeplugme.ui

import androidx.compose.foundation.layout.RowScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Save
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import me.faulk.codeplugme.library.LibraryEntry
import me.faulk.codeplugme.state.AppViewModel
import me.faulk.codeplugme.state.CodeplugSection
import me.faulk.codeplugme.state.CodeplugState
import me.faulk.codeplugme.state.RadioScreenState
import me.faulk.codeplugme.state.addChannel
import me.faulk.codeplugme.state.addContact
import me.faulk.codeplugme.state.addGroupList
import me.faulk.codeplugme.state.addQytChannel
import me.faulk.codeplugme.state.addRadioId
import me.faulk.codeplugme.state.addScanList
import me.faulk.codeplugme.state.addZone
import me.faulk.codeplugme.state.bulkUpdateChannels
import me.faulk.codeplugme.state.bulkUpdateQytChannels
import me.faulk.codeplugme.state.remove
import me.faulk.codeplugme.state.update
import me.faulk.codeplugme.state.updateQytBands
import me.faulk.codeplugme.state.updateQytSettings
import me.faulk.codeplugme.ui.channels.BulkChannelEditorScreen
import me.faulk.codeplugme.ui.channels.BulkQytChannelEditorScreen
import me.faulk.codeplugme.ui.channels.ChannelEditorScreen
import me.faulk.codeplugme.ui.channels.ChannelsScreen
import me.faulk.codeplugme.ui.channels.QytChannelEditorScreen
import me.faulk.codeplugme.ui.channels.QytChannelsScreen
import me.faulk.codeplugme.ui.common.EmptyState
import me.faulk.codeplugme.ui.config.AprsScreen
import me.faulk.codeplugme.ui.config.QytBandsScreen
import me.faulk.codeplugme.ui.config.QytSettingsScreen
import me.faulk.codeplugme.ui.home.RadioScreen
import me.faulk.codeplugme.ui.importer.ImportViewModel
import me.faulk.codeplugme.ui.importer.RepeaterImportScreen
import me.faulk.codeplugme.ui.members.MemberCandidate
import me.faulk.codeplugme.ui.members.MemberListScreen
import me.faulk.codeplugme.ui.members.MemberPickerScreen
import me.faulk.codeplugme.ui.records.ContactEditorScreen
import me.faulk.codeplugme.ui.records.ContactsScreen
import me.faulk.codeplugme.ui.records.GroupListsScreen
import me.faulk.codeplugme.ui.records.RadioIdEditorScreen
import me.faulk.codeplugme.ui.records.RadioIdsScreen
import me.faulk.codeplugme.ui.records.ScanListEditorScreen
import me.faulk.codeplugme.ui.records.ScanListsScreen
import me.faulk.codeplugme.ui.records.SimpleNameEditorScreen
import me.faulk.codeplugme.ui.records.ZonesScreen
import me.faulk.codeplugme.ui.common.formatSlot

/**
 * Renders whichever destination is current.
 *
 * One big `when` rather than a route table, because every branch needs the same
 * handful of things (the state, the store, the back stack) and threading them
 * through a registry would add indirection without removing any of the wiring.
 */
@Composable
fun AppContent(
    vm: AppViewModel,
    current: Destination,
    radio: RadioScreenState,
    codeplug: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    push: (Destination) -> Unit,
    pop: () -> Unit,
    onWriteRequest: (LibraryEntry) -> Unit,
    onRenameRequest: (LibraryEntry) -> Unit,
    onDeleteRequest: (LibraryEntry) -> Unit,
    onExportRequest: (LibraryEntry) -> Unit,
    onImportRequest: () -> Unit,
    onSaveAsRequest: () -> Unit,
    onDiscardChangesRequest: () -> Unit,
    scope: CoroutineScope,
) {
    /** Run a suspend store operation from a click. */
    fun edit(block: suspend () -> Unit) = scope.launch { block() }

    /** Open a codeplug and jump to its channels, which is what "Edit" means. */
    fun openForEditing(entry: LibraryEntry) = scope.launch {
        vm.store.open(entry.id)
    }

    when (current) {
        Destination.Radio -> RadioScreen(
            state = radio,
            codeplug = codeplug,
            onSave = vm::saveOpen,
            onSaveAs = onSaveAsRequest,
            onDiscard = onDiscardChangesRequest,
            onSelectDevice = vm::selectDevice,
            onChooseModel = vm::chooseModel,
            onRefresh = vm::refreshDevices,
            onRead = vm::readFromRadio,
            onWrite = onWriteRequest,
            onOpen = { openForEditing(it) },
            onImport = onImportRequest,
            onExport = onExportRequest,
            onRename = onRenameRequest,
            onDuplicate = { vm.duplicate(it.id) },
            onDelete = onDeleteRequest,
        )

        Destination.Config -> ConfigDestination(vm, codeplug, push, scope)

        Destination.Channels -> ChannelsDestination(
            vm, codeplug, selection, onSelectionChange, push, scope
        )

        Destination.More -> MoreScreen(
            state = codeplug,
            onOpenSection = { push(Destination.Records(it)) },
            onImportRepeaters = { push(Destination.RepeaterImport) },
        )

        Destination.RepeaterImport -> {
            // One for the activity, so the search and results survive leaving
            // the screen. It drops results itself when another codeplug opens.
            val importVm: ImportViewModel = viewModel(key = "repeater-import")
            RepeaterImportScreen(
                vm = importVm,
                entryId = codeplug.entryId,
                baseImage = { withContext(Dispatchers.IO) { vm.store.stagedBytes() } },
                baseName = codeplug.entryName,
                supportsDmr = codeplug.capabilities?.supportsDmr == true,
                addToCodeplug = { picked, options -> vm.store.addRepeaters(picked, options) },
                onAdded = pop,
                onBuiltNew = { image ->
                    vm.saveGenerated("${importVm.state.value.zoneName.trim()} repeaters", image)
                    pop()
                },
                onBack = pop,
            )
        }

        is Destination.Records -> RecordsDestination(
            vm, codeplug, current.section, selection, onSelectionChange, push, pop, scope
        )

        is Destination.Editor -> EditorDestination(vm, codeplug, current, push, pop, scope)

        is Destination.Members -> MembersDestination(vm, codeplug, current, push, pop, scope)

        is Destination.MemberPicker -> MemberPickerDestination(vm, codeplug, current, pop, scope)

        is Destination.BulkChannels -> {
            val slots = current.slots
            if (codeplug.modelId == "qyt-kt5000") {
                BulkQytChannelEditorScreen(
                    count = slots.size,
                    onApply = { u ->
                        edit { vm.store.bulkUpdateQytChannels(slots, u) }
                        onSelectionChange(emptySet())
                        pop()
                    },
                    onBack = pop,
                )
            } else {
                BulkChannelEditorScreen(
                    count = slots.size,
                    contacts = codeplug.contacts,
                    groupLists = codeplug.groupLists,
                    radioIds = codeplug.radioIds,
                    capabilities = codeplug.capabilities,
                    onApply = { u ->
                        edit { vm.store.bulkUpdateChannels(slots, u) }
                        onSelectionChange(emptySet())
                        pop()
                    },
                    onBack = pop,
                )
            }
        }
    }
}

/**
 * Priority 2: the open codeplug's radio configuration.
 *
 * A radio with one config section (the AnyTone, whose only one is the read-only
 * APRS view) shows it directly, because an index page listing a single entry is
 * just a tap in the way. A radio with several (the QYT: settings and VFO
 * channels) shows the index, since neither is obviously the one you meant.
 */
@Composable
private fun ConfigDestination(
    vm: AppViewModel,
    codeplug: CodeplugState,
    push: (Destination) -> Unit,
    scope: CoroutineScope,
) {
    if (!codeplug.isOpen) {
        EmptyState(
            "No codeplug open",
            "Open one from the Radio tab to change this radio's settings.",
        )
        return
    }

    val configSections = codeplug.sections.filter { it.isConfig }
    when (configSections.size) {
        0 -> EmptyState("No settings", "This radio exposes no settings this app can show.")

        1 -> ConfigSectionContent(vm, codeplug, configSections.single(), onBack = null, scope)

        else -> ConfigIndex(
            sections = configSections,
            state = codeplug,
            onOpen = { push(Destination.Records(it)) },
        )
    }
}

/** The index of a radio's config sections, when it has more than one. */
@Composable
private fun ConfigIndex(
    sections: List<CodeplugSection>,
    state: CodeplugState,
    onOpen: (CodeplugSection) -> Unit,
) = SectionIndex(
    sections = sections,
    subtitle = { section ->
        when (section) {
            CodeplugSection.QYT_SETTINGS -> "Squelch, timeout, backlight, beep, scan resume"
            CodeplugSection.QYT_BANDS -> "VFO A and B"
            CodeplugSection.APRS -> "Read-only"
            else -> ""
        }
    },
    hasUnsaved = { state.hasUnsavedChanges(it) },
    onOpen = onOpen,
)

/**
 * One config section's screen.
 *
 * [onBack] is null when this is the tab's own content rather than a pushed
 * screen, because a top-level tab has nothing to go back to.
 */
@Composable
private fun ConfigSectionContent(
    vm: AppViewModel,
    codeplug: CodeplugState,
    section: CodeplugSection,
    onBack: (() -> Unit)?,
    scope: CoroutineScope,
) = when (section) {
    CodeplugSection.QYT_SETTINGS -> QytSettingsScreen(
        settings = codeplug.qytSettings,
        staticMessage = codeplug.qytStaticMessage,
        onApply = { s, msg -> scope.launch { vm.store.updateQytSettings(s, msg) } },
        onBack = onBack,
    )

    CodeplugSection.QYT_BANDS -> QytBandsScreen(
        bands = codeplug.qytBands,
        capabilities = codeplug.capabilities,
        onApply = { bands -> scope.launch { vm.store.updateQytBands(bands) } },
        onBack = onBack,
    )

    // The AnyTone has no writable global settings; APRS is all there is to
    // show, and it is read-only. See the note inside AprsScreen.
    CodeplugSection.APRS -> AprsScreen(codeplug.aprs, onBack = onBack)

    else -> EmptyState("Not available", "This radio has no ${section.title} section.")
}

/** Priority 3: channels, with bulk edit as the leading selection action. */
@Composable
private fun ChannelsDestination(
    vm: AppViewModel,
    codeplug: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    push: (Destination) -> Unit,
    scope: CoroutineScope,
) {
    if (!codeplug.isOpen) {
        EmptyState("No codeplug open", "Open one from the Radio tab to edit its channels.")
        return
    }
    val qyt = CodeplugSection.QYT_CHANNELS in codeplug.sections
    val section = if (qyt) CodeplugSection.QYT_CHANNELS else CodeplugSection.CHANNELS

    // Channels is a bottom-bar tab with an empty back stack behind it, so it
    // gets no back arrow; what the space is worth is a save, since this is
    // where the edits that need saving are made.
    val actions: @Composable RowScope.() -> Unit = {
        if (codeplug.isDirty) {
            IconButton(onClick = vm::saveOpen) {
                Icon(Icons.Default.Save, contentDescription = "Save codeplug")
            }
        }
    }

    if (qyt) {
        QytChannelsScreen(
            channels = codeplug.qytChannels,
            isUnsaved = { codeplug.isUnsaved(section, it) },
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch {
                    vm.store.addQytChannel()?.let { push(Destination.Editor(section, it)) }
                }
            },
            onBulkEdit = { push(Destination.BulkChannels(it)) },
            onDelete = { slots ->
                scope.launch { vm.store.remove(section, slots) }
                onSelectionChange(emptySet())
            },
            onBack = null,
            actions = actions,
        )
    } else {
        ChannelsScreen(
            channels = codeplug.channels,
            capabilities = codeplug.capabilities,
            isUnsaved = { codeplug.isUnsaved(section, it) },
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch {
                    vm.store.addChannel()?.let { push(Destination.Editor(section, it)) }
                }
            },
            onBulkEdit = { push(Destination.BulkChannels(it)) },
            onDelete = { slots ->
                scope.launch { vm.store.remove(section, slots) }
                onSelectionChange(emptySet())
            },
            onBack = null,
            actions = actions,
        )
    }
}

/** The record families behind More, plus the QYT VFO screen. */
@Composable
private fun RecordsDestination(
    vm: AppViewModel,
    codeplug: CodeplugState,
    section: CodeplugSection,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    push: (Destination) -> Unit,
    pop: () -> Unit,
    scope: CoroutineScope,
) {
    fun deleteSelected(slots: Set<Int>) {
        scope.launch { vm.store.remove(section, slots) }
        onSelectionChange(emptySet())
    }

    when (section) {
        CodeplugSection.ZONES -> ZonesScreen(
            state = codeplug,
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch { vm.store.addZone()?.let { push(Destination.Editor(section, it)) } }
            },
            onDelete = ::deleteSelected,
            onBack = pop,
        )

        CodeplugSection.SCAN_LISTS -> ScanListsScreen(
            state = codeplug,
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch { vm.store.addScanList()?.let { push(Destination.Editor(section, it)) } }
            },
            onDelete = ::deleteSelected,
            onBack = pop,
        )

        CodeplugSection.CONTACTS -> ContactsScreen(
            state = codeplug,
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch { vm.store.addContact()?.let { push(Destination.Editor(section, it)) } }
            },
            onDelete = ::deleteSelected,
            onBack = pop,
        )

        CodeplugSection.GROUP_LISTS -> GroupListsScreen(
            state = codeplug,
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch { vm.store.addGroupList()?.let { push(Destination.Editor(section, it)) } }
            },
            onDelete = ::deleteSelected,
            onBack = pop,
        )

        CodeplugSection.RADIO_IDS -> RadioIdsScreen(
            state = codeplug,
            selection = selection,
            onSelectionChange = onSelectionChange,
            onOpen = { push(Destination.Editor(section, it.index)) },
            onAdd = {
                scope.launch { vm.store.addRadioId()?.let { push(Destination.Editor(section, it)) } }
            },
            onDelete = ::deleteSelected,
            onBack = pop,
        )

        // The config sections are reachable both as the Config tab's own content
        // and as a pushed screen from its index, so they share one renderer.
        CodeplugSection.QYT_SETTINGS, CodeplugSection.QYT_BANDS, CodeplugSection.APRS ->
            ConfigSectionContent(vm, codeplug, section, onBack = pop, scope = scope)

        else -> EmptyState("Not available", "This radio has no ${section.title} section.")
    }
}

/** One record's editor, chosen by the section it belongs to. */
@Composable
private fun EditorDestination(
    vm: AppViewModel,
    codeplug: CodeplugState,
    dest: Destination.Editor,
    push: (Destination) -> Unit,
    pop: () -> Unit,
    scope: CoroutineScope,
) {
    /** Save through the store, then leave the editor. */
    fun save(block: suspend () -> Unit) {
        scope.launch { block() }
        pop()
    }

    when (dest.section) {
        CodeplugSection.CHANNELS -> {
            val ch = codeplug.channels.firstOrNull { it.index == dest.slot } ?: return pop()
            ChannelEditorScreen(
                channel = ch,
                contacts = codeplug.contacts,
                groupLists = codeplug.groupLists,
                radioIds = codeplug.radioIds,
                scanLists = codeplug.scanLists,
                capabilities = codeplug.capabilities,
                onSave = { edited -> save { vm.store.update(edited) } },
                onBack = pop,
            )
        }

        CodeplugSection.QYT_CHANNELS -> {
            val ch = codeplug.qytChannels.firstOrNull { it.index == dest.slot } ?: return pop()
            QytChannelEditorScreen(
                channel = ch,
                nameLimit = codeplug.capabilities?.channelNameLength ?: 7,
                powerOptions = codeplug.capabilities?.powerOptions.orEmpty(),
                bandwidthOptions = codeplug.capabilities?.bandwidthOptions.orEmpty(),
                onSave = { edited -> save { vm.store.update(edited) } },
                onBack = pop,
            )
        }

        CodeplugSection.ZONES -> {
            val z = codeplug.zones.firstOrNull { it.index == dest.slot } ?: return pop()
            SimpleNameEditorScreen(
                title = "Zone ${formatSlot(z.index)}",
                name = z.name,
                memberLabel = "Channels",
                memberCount = z.channels.size,
                onSave = { name -> save { vm.store.update(z.copy(name = name)) } },
                onEditMembers = { push(Destination.Members(dest.section, dest.slot)) },
                onBack = pop,
            )
        }

        CodeplugSection.GROUP_LISTS -> {
            val g = codeplug.groupLists.firstOrNull { it.index == dest.slot } ?: return pop()
            SimpleNameEditorScreen(
                title = "Group list ${formatSlot(g.index)}",
                name = g.name,
                memberLabel = "Contacts",
                memberCount = g.members.size,
                onSave = { name -> save { vm.store.update(g.copy(name = name)) } },
                onEditMembers = { push(Destination.Members(dest.section, dest.slot)) },
                onBack = pop,
            )
        }

        CodeplugSection.SCAN_LISTS -> {
            val s = codeplug.scanLists.firstOrNull { it.index == dest.slot } ?: return pop()
            ScanListEditorScreen(
                scanList = s,
                onSave = { edited -> save { vm.store.update(edited) } },
                onEditMembers = { push(Destination.Members(dest.section, dest.slot)) },
                onBack = pop,
            )
        }

        CodeplugSection.CONTACTS -> {
            val c = codeplug.contacts.firstOrNull { it.index == dest.slot } ?: return pop()
            ContactEditorScreen(
                contact = c,
                onSave = { edited -> save { vm.store.update(edited) } },
                onBack = pop,
            )
        }

        CodeplugSection.RADIO_IDS -> {
            val r = codeplug.radioIds.firstOrNull { it.index == dest.slot } ?: return pop()
            RadioIdEditorScreen(
                radioId = r,
                onSave = { edited -> save { vm.store.update(edited) } },
                onBack = pop,
            )
        }

        else -> pop()
    }
}

/** The candidates and current members for a membership-editing destination. */
private fun membership(
    codeplug: CodeplugState,
    section: CodeplugSection,
    slot: Int,
): Pair<List<MemberCandidate>, List<Int>>? = when (section) {
    CodeplugSection.ZONES -> {
        val z = codeplug.zones.firstOrNull { it.index == slot } ?: return null
        codeplug.channels.map { MemberCandidate(it.index, it.name) } to z.channels
    }

    CodeplugSection.SCAN_LISTS -> {
        val s = codeplug.scanLists.firstOrNull { it.index == slot } ?: return null
        codeplug.channels.map { MemberCandidate(it.index, it.name) } to s.members
    }

    CodeplugSection.GROUP_LISTS -> {
        val g = codeplug.groupLists.firstOrNull { it.index == slot } ?: return null
        // Only named contacts are worth offering: the AnyTone always reports
        // its full 10,000 slots, and an unnamed one is an empty slot.
        codeplug.contacts.filter { it.name.isNotBlank() }
            .map { MemberCandidate(it.index, it.name) } to g.members
    }

    else -> null
}

/** Commit a changed membership list back through the store. */
private fun applyMembership(
    vm: AppViewModel,
    codeplug: CodeplugState,
    section: CodeplugSection,
    slot: Int,
    members: List<Int>,
    scope: CoroutineScope,
) {
    scope.launch {
        when (section) {
            CodeplugSection.ZONES -> codeplug.zones.firstOrNull { it.index == slot }
                ?.let { vm.store.update(it.copy(channels = members)) }

            CodeplugSection.SCAN_LISTS -> codeplug.scanLists.firstOrNull { it.index == slot }
                ?.let { vm.store.update(it.copy(members = members)) }

            CodeplugSection.GROUP_LISTS -> codeplug.groupLists.firstOrNull { it.index == slot }
                ?.let { vm.store.update(it.copy(members = members)) }

            else -> Unit
        }
    }
}

/** A record's membership list, with reordering and removal. */
@Composable
private fun MembersDestination(
    vm: AppViewModel,
    codeplug: CodeplugState,
    dest: Destination.Members,
    push: (Destination) -> Unit,
    pop: () -> Unit,
    scope: CoroutineScope,
) {
    val (candidates, members) = membership(codeplug, dest.section, dest.slot) ?: return pop()
    MemberListScreen(
        title = dest.section.title,
        candidates = candidates,
        members = members,
        onMembersChange = {
            applyMembership(vm, codeplug, dest.section, dest.slot, it, scope)
        },
        onAdd = { push(Destination.MemberPicker(dest.section, dest.slot)) },
        onBack = pop,
    )
}

/** Picking records to add to a membership list. */
@Composable
private fun MemberPickerDestination(
    vm: AppViewModel,
    codeplug: CodeplugState,
    dest: Destination.MemberPicker,
    pop: () -> Unit,
    scope: CoroutineScope,
) {
    val (candidates, members) = membership(codeplug, dest.section, dest.slot) ?: return pop()
    MemberPickerScreen(
        title = "Add to ${dest.section.title.lowercase()}",
        candidates = candidates,
        members = members,
        onConfirm = {
            applyMembership(vm, codeplug, dest.section, dest.slot, it, scope)
            pop()
        },
        onBack = pop,
    )
}
