package me.faulk.codeplugme.ui.importer

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.MyLocation
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.io.File
import java.util.Locale
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.GeoBuildOptions
import me.faulk.codeplugme.model.Repeater
import me.faulk.codeplugme.model.RepeaterMode
import me.faulk.codeplugme.model.RepeaterSource
import me.faulk.codeplugme.model.Tone

/**
 * Add repeaters from databases around a place to the open codeplug.
 *
 * Top to bottom: where, how far, which databases, clean or merge, then the
 * results to pick from. Nothing is picked until the user picks it, and the
 * counter at the bottom says how many of those fit.
 *
 * "Add to codeplug" (the default) stages the picks into the open codeplug
 * through [addToCodeplug] as one undoable change, then calls [onAdded]. "Save
 * as new" leaves the open codeplug alone and hands a new image to
 * [onBuiltNew].
 *
 * [baseImage] is the open codeplug's staged bytes, read each time the screen
 * opens.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RepeaterImportScreen(
    vm: ImportViewModel,
    entryId: String?,
    baseImage: suspend () -> ByteArray,
    baseName: String,
    supportsDmr: Boolean,
    addToCodeplug: suspend (List<Repeater>, GeoBuildOptions) -> Boolean,
    onAdded: () -> Unit,
    onBuiltNew: (ByteArray) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val state by vm.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var editingRepeaterBook by remember { mutableStateOf(false) }

    /** Fill the location from the phone's own fix. */
    fun locate() = scope.launch {
        vm.setLocating(true)
        val fix = LocationLookup.current(context)
        if (fix == null) {
            vm.setError("No location fix yet. Type a place instead.")
        } else {
            vm.setLocation(LocationLookup.reverse(context, fix.latitude, fix.longitude))
            vm.setError(null)
        }
    }

    /** Resolve what the user typed into a location. */
    fun search() = scope.launch {
        val placeQuery = state.placeQuery.trim()
        if (placeQuery.isBlank()) return@launch
        vm.setLocating(true)
        val found = LocationLookup.forward(context, placeQuery)
        if (found == null) vm.setError("Could not find \"$placeQuery\".")
        else { vm.setLocation(found); vm.setError(null) }
    }

    val permission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted -> if (granted) locate() }

    LaunchedEffect(Unit) {
        vm.configure(entryId, baseImage(), supportsDmr, RepeaterBookPrefs.load(context) != null)
        if (state.location == null) {
            val granted = ContextCompat.checkSelfPermission(
                context, Manifest.permission.ACCESS_COARSE_LOCATION
            ) == PackageManager.PERMISSION_GRANTED
            if (granted) locate() else permission.launch(Manifest.permission.ACCESS_COARSE_LOCATION)
        }
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Nearby repeaters") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = { editingRepeaterBook = true }) {
                        Icon(Icons.Default.Settings, contentDescription = "RepeaterBook settings")
                    }
                },
            )
        },
        bottomBar = {
            BuildBar(
                state = state,
                onAdd = { scope.launch { if (vm.addToCodeplug(addToCodeplug)) onAdded() } },
                onSaveNew = { scope.launch { vm.build()?.let(onBuiltNew) } },
            )
        },
    ) { padding ->
        LazyColumn(Modifier.padding(padding).fillMaxSize()) {
            item {
                Column(
                    Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    LocationSection(
                        state = state,
                        query = state.placeQuery,
                        onQueryChange = vm::setPlaceQuery,
                        onSearch = { search() },
                        onLocate = {
                            val granted = ContextCompat.checkSelfPermission(
                                context, Manifest.permission.ACCESS_COARSE_LOCATION
                            ) == PackageManager.PERMISSION_GRANTED
                            if (granted) locate()
                            else permission.launch(Manifest.permission.ACCESS_COARSE_LOCATION)
                        },
                    )

                    Text("Radius: ${state.radiusKm.roundToInt()} km (${(state.radiusKm * 0.621f).roundToInt()} mi)")
                    Slider(
                        value = state.radiusKm,
                        onValueChange = { vm.setRadius(it.roundToInt().toFloat()) },
                        valueRange = 5f..150f,
                    )

                    SourceToggles(state, vm::toggleSource, onSetUpRepeaterBook = { editingRepeaterBook = true })

                    ModeChoice(state.mode, vm::setMode)

                    OutlinedTextField(
                        value = state.zoneName,
                        onValueChange = vm::setZoneName,
                        label = { Text("Zone name") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )

                    Note(
                        if (state.mode == BuildMode.CLEAN) {
                            "Add to codeplug replaces $baseName's channels as an unsaved change you can undo. "
                        } else {
                            "Add to codeplug adds to $baseName as an unsaved change you can undo. "
                        } + "Save as new leaves $baseName alone and makes a new codeplug instead."
                    )
                    state.capacity?.blockedReason?.let {
                        Text(it, color = MaterialTheme.colorScheme.error)
                    }
                    state.capacity?.scanListLimit?.let {
                        Note("The scan list holds the nearest $it of the channels you pick.")
                    }

                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Button(
                            onClick = { vm.fetch(RepeaterBookPrefs.load(context), File(context.cacheDir, "geo")) },
                            enabled = state.canFetch,
                        ) { Text("Fetch repeaters") }
                        if (state.loading) {
                            CircularProgressIndicator(Modifier.padding(start = 16.dp).padding(4.dp))
                        }
                    }
                    state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                }
            }

            if (state.results.isNotEmpty()) {
                item { ResultControls(state, vm) }
                items(state.visible, key = { it.index }) { (index, repeater) ->
                    RepeaterRow(
                        repeater = repeater,
                        checked = index in state.selected,
                        selectable = state.isSelectable(index),
                        onToggle = { vm.toggle(index) },
                    )
                    HorizontalDivider()
                }
                if (state.showsRepeaterBookCredit) {
                    item {
                        Text(
                            "Data courtesy of RepeaterBook.com",
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.padding(16.dp),
                        )
                    }
                }
            } else if (!state.loading && state.capacity != null && state.error == null) {
                item { Note("Fetch to see repeaters here.", Modifier.padding(16.dp)) }
            }
        }
    }

    if (state.overLimitAlert) {
        val max = state.capacity?.maxSelectable ?: 0
        AlertDialog(
            onDismissRequest = vm::dismissOverLimit,
            title = { Text("Too many channels") },
            text = {
                Text(
                    "You selected ${state.selectedCount} channels but only $max fit. " +
                        "Deselect ${state.excess}."
                )
            },
            confirmButton = { TextButton(onClick = vm::dismissOverLimit) { Text("OK") } },
        )
    }

    if (editingRepeaterBook) {
        RepeaterBookDialog(
            onSave = { token, contact ->
                RepeaterBookPrefs.save(context, token, contact)
                vm.setRepeaterBookAuth(RepeaterBookPrefs.load(context) != null)
                editingRepeaterBook = false
            },
            onDismiss = { editingRepeaterBook = false },
        )
    }
}

/** The current place, the "use my location" button, and the place search. */
@Composable
private fun LocationSection(
    state: ImportState,
    query: String,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    onLocate: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Location", style = MaterialTheme.typography.labelMedium)
            Text(
                when {
                    state.locating -> "Finding your location..."
                    state.location != null -> state.location.label
                    else -> "Not set"
                },
                style = MaterialTheme.typography.bodyLarge,
            )
        }
        IconButton(onClick = onLocate, enabled = !state.locating) {
            Icon(Icons.Default.MyLocation, contentDescription = "Use my location")
        }
    }
    OutlinedTextField(
        value = query,
        onValueChange = onQueryChange,
        label = { Text("Place, ZIP, or lat, lon") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { onSearch() }),
        trailingIcon = {
            IconButton(onClick = onSearch) { Icon(Icons.Default.Search, contentDescription = "Search") }
        },
        modifier = Modifier.fillMaxWidth(),
    )
}

/** One chip per database. RadioID is absent without DMR; RepeaterBook needs setup. */
@Composable
private fun SourceToggles(
    state: ImportState,
    onToggle: (RepeaterSource) -> Unit,
    onSetUpRepeaterBook: () -> Unit,
) {
    Column {
        Text("Sources", style = MaterialTheme.typography.labelMedium)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SourceChip("Hearham", RepeaterSource.HEARHAM, state, onToggle)
            if (state.supportsDmr) SourceChip("RadioID DMR", RepeaterSource.RADIOID, state, onToggle)
            SourceChip(
                "RepeaterBook", RepeaterSource.REPEATERBOOK, state, onToggle,
                enabled = state.hasRepeaterBookAuth,
            )
        }
        if (!state.hasRepeaterBookAuth) {
            Text(
                "RepeaterBook needs your own token and contact email. Set them up",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.clickable(onClick = onSetUpRepeaterBook),
            )
        }
    }
}

/** A source toggle, shown on only when it will actually be sent. */
@Composable
private fun SourceChip(
    label: String,
    source: RepeaterSource,
    state: ImportState,
    onToggle: (RepeaterSource) -> Unit,
    enabled: Boolean = true,
) {
    FilterChip(
        selected = source in state.effectiveSources,
        onClick = { onToggle(source) },
        label = { Text(label) },
        enabled = enabled,
    )
}

/** Clean or merge, with what each one does. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ModeChoice(mode: BuildMode, onChange: (BuildMode) -> Unit) {
    Column {
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            BuildMode.entries.forEachIndexed { i, m ->
                SegmentedButton(
                    selected = mode == m,
                    onClick = { onChange(m) },
                    shape = SegmentedButtonDefaults.itemShape(i, BuildMode.entries.size),
                ) { Text(if (m == BuildMode.CLEAN) "Clean" else "Merge") }
            }
        }
        Note(
            if (mode == BuildMode.CLEAN) {
                "Keeps settings and contacts but replaces all channels, zones, and scan lists."
            } else {
                "Adds the picked repeaters to the channels, zones, and scan lists already there."
            },
            Modifier.padding(top = 4.dp),
        )
    }
}

/** Select all or none, and the band and mode filters. */
@Composable
private fun ResultControls(state: ImportState, vm: ImportViewModel) {
    Column(Modifier.padding(horizontal = 16.dp)) {
        HorizontalDivider()
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "${state.results.size} found",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = vm::selectAll) { Text("Select all") }
            TextButton(onClick = vm::selectNone) { Text("Select none") }
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            BandFilter.entries.forEach { f ->
                FilterChip(
                    selected = state.bandFilter == f,
                    onClick = { vm.setBandFilter(f) },
                    label = { Text(f.label) },
                )
            }
            if (state.supportsDmr) {
                ModeFilter.entries.forEach { f ->
                    FilterChip(
                        selected = state.modeFilter == f,
                        onClick = { vm.setModeFilter(f) },
                        label = { Text(f.label) },
                    )
                }
            }
        }
    }
}

/** One result: distance, callsign, frequencies, tone or color code, mode, place. */
@Composable
private fun RepeaterRow(
    repeater: Repeater,
    checked: Boolean,
    selectable: Boolean,
    onToggle: () -> Unit,
) {
    val distance = repeater.distanceKm?.let { "%.1f km".format(Locale.ROOT, it) }
    ListItem(
        leadingContent = {
            Checkbox(checked = checked, onCheckedChange = { onToggle() }, enabled = selectable)
        },
        headlineContent = {
            Text(listOfNotNull(distance, repeater.callsign).joinToString("  "))
        },
        supportingContent = {
            Column {
                Text(
                    "${mhz(repeater.rxHz)} / ${mhz(repeater.txHz)}  ${signalling(repeater)}  " +
                        if (repeater.mode == RepeaterMode.DMR) "DMR" else "FM"
                )
                Text(repeater.location, style = MaterialTheme.typography.bodySmall)
                if (!selectable) {
                    Text(
                        "Already in codeplug",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        },
        modifier = Modifier.clickable(enabled = selectable, onClick = onToggle),
    )
}

/** The sticky counter, Save as new, and the default Add to codeplug. */
@Composable
private fun BuildBar(state: ImportState, onAdd: () -> Unit, onSaveNew: () -> Unit) {
    Surface(tonalElevation = 3.dp) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "${state.selectedCount} selected / ${state.capacity?.maxSelectable ?: "?"} available",
                    color = if (state.isOverLimit) MaterialTheme.colorScheme.error
                    else MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                if (state.building) CircularProgressIndicator(Modifier.padding(4.dp))
            }
            Row(
                Modifier.fillMaxWidth().padding(top = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
            ) {
                OutlinedButton(onClick = onSaveNew, enabled = state.canBuild) { Text("Save as new") }
                Button(onClick = onAdd, enabled = state.canBuild) { Text("Add to codeplug") }
            }
        }
    }
}

/** Token and contact email for RepeaterBook, saved on the phone only. */
@Composable
private fun RepeaterBookDialog(onSave: (String, String) -> Unit, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val saved = remember { RepeaterBookPrefs.load(context) }
    var token by remember { mutableStateOf(saved?.token.orEmpty()) }
    var contact by remember { mutableStateOf(saved?.contact.orEmpty()) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("RepeaterBook") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Note(
                    "RepeaterBook needs an app token from repeaterbook.com, tied to a contact " +
                        "email it sees in each request. Both stay on this phone."
                )
                OutlinedTextField(
                    value = token,
                    onValueChange = { token = it },
                    label = { Text("Token") },
                    singleLine = true,
                    supportingText = repeaterBookTokenWarning(token)?.let { { Text(it) } },
                )
                OutlinedTextField(
                    value = contact,
                    onValueChange = { contact = it },
                    label = { Text("Contact email") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                )
            }
        },
        confirmButton = { TextButton(onClick = { onSave(token, contact) }) { Text("Save") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/**
 * A warning when [token] is not an export API token, or null. The export API
 * takes only app tokens (`rbuapp_` or `app_`); a bare value is usually the
 * private account token, which RepeaterBook rejects there as unknown.
 */
internal fun repeaterBookTokenWarning(token: String): String? {
    val t = token.trim()
    if (t.isEmpty() || t.startsWith("rbuapp_") || t.startsWith("app_")) return null
    return "This looks like a private account token, which RepeaterBook's export API " +
        "doesn't accept. It needs an app token starting with rbuapp_ (or app_)."
}

/** Small secondary text. */
@Composable
private fun Note(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = modifier,
    )
}

/** Hz as "146.9400". */
private fun mhz(hz: Long): String = "%.4f".format(Locale.ROOT, hz / 1_000_000.0)

/** The tone to open an analog repeater, or a DMR repeater's color code. */
internal fun signalling(r: Repeater): String = when {
    r.mode == RepeaterMode.DMR -> r.colorCode?.let { "CC $it" } ?: "CC ?"
    else -> when (val t = r.txTone) {
        Tone.None -> "No tone"
        is Tone.Ctcss -> "%.1f Hz".format(Locale.ROOT, t.hzTenths / 10.0)
        is Tone.Dcs -> "D%03d%s".format(Locale.ROOT, t.code, if (t.inverted) "I" else "N")
    }
}
