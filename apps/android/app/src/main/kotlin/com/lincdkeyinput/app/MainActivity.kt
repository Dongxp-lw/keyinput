package com.lincdkeyinput.app

import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lincdkeyinput.app.ui.screens.EntryDetailScreen
import com.lincdkeyinput.app.ui.screens.EntryEditScreen
import com.lincdkeyinput.app.ui.screens.OnboardingScreen
import com.lincdkeyinput.app.ui.screens.SettingsScreen
import com.lincdkeyinput.app.ui.screens.UnlockScreen
import com.lincdkeyinput.app.ui.screens.VaultListScreen
import com.lincdkeyinput.app.ui.theme.VaultTheme

/** L4-APP 主入口：Compose 宿主（FragmentActivity 以支持后续生物识别 BiometricPrompt）。 */
class MainActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            VaultTheme {
                VaultApp()
            }
        }
    }
}

@Composable
private fun VaultApp(vm: VaultViewModel = viewModel()) {
    val uiState by vm.uiState.collectAsStateWithLifecycle()
    val busy by vm.busy.collectAsStateWithLifecycle()
    val message by vm.message.collectAsStateWithLifecycle()

    val snackbarHostState = remember { SnackbarHostState() }
    val activity = LocalContext.current as? FragmentActivity

    // 回前台时同步锁定状态（后台已自动锁定则切回解锁界面）。
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) vm.syncLockState()
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    LaunchedEffect(message) {
        message?.let {
            snackbarHostState.showSnackbar(it)
            vm.consumeMessage()
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        when (uiState) {
            VaultUiState.Loading -> CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            VaultUiState.Onboarding -> OnboardingScreen(busy = busy, onCreate = vm::createVault)
            VaultUiState.Locked -> {
                val failedAttempts by vm.failedAttempts.collectAsStateWithLifecycle()
                val lockoutRemainingSec by vm.lockoutRemainingSec.collectAsStateWithLifecycle()
                UnlockScreen(
                    busy = busy,
                    biometricAvailable = vm.biometricUnlockReady(),
                    failedAttempts = failedAttempts,
                    lockoutRemainingSec = lockoutRemainingSec,
                    onUnlock = vm::unlock,
                    onBiometric = { activity?.let { vm.biometricUnlock(it) } },
                )
            }
            VaultUiState.Unlocked -> UnlockedRoot(vm, activity)
        }
        SnackbarHost(snackbarHostState, modifier = Modifier.align(Alignment.BottomCenter))
    }
}

@Composable
private fun UnlockedRoot(vm: VaultViewModel, activity: FragmentActivity?) {
    val screen by vm.screen.collectAsStateWithLifecycle()
    val entries by vm.entries.collectAsStateWithLifecycle()
    val query by vm.query.collectAsStateWithLifecycle()
    val busy by vm.busy.collectAsStateWithLifecycle()
    val biometricEnabled by vm.biometricEnabled.collectAsStateWithLifecycle()

    when (val s = screen) {
        Screen.List -> VaultListScreen(
            entries = entries,
            query = query,
            onQueryChange = vm::onQueryChange,
            onOpen = vm::openEntry,
            onAdd = vm::newEntry,
            onSettings = vm::openSettings,
            onLock = vm::lock,
        )
        is Screen.Detail -> {
            val entry = vm.getEntry(s.entryId)
            if (entry == null) {
                LaunchedEffect(s.entryId) { vm.back() }
            } else {
                EntryDetailScreen(
                    entry = entry,
                    onBack = vm::back,
                    onEdit = { vm.editEntry(s.entryId) },
                    onDelete = { vm.deleteEntry(s.entryId) },
                    onCopy = { fieldId, label -> vm.copyField(s.entryId, fieldId, label) },
                    totpFor = vm::totpFor,
                )
            }
        }
        is Screen.Edit -> EntryEditScreen(
            initial = s.entryId?.let { vm.getEntry(it) },
            onBack = vm::back,
            onSave = vm::saveEntry,
            onGenerate = vm::generate,
        )
        Screen.Settings -> SettingsScreen(
            busy = busy,
            onBack = vm::back,
            onChangePassword = vm::changePassword,
            onExport = vm::exportTo,
            onImport = vm::importFrom,
            biometricSupported = vm.biometricHardwareAvailable(),
            biometricEnabled = biometricEnabled,
            onEnableBiometric = { pw -> activity?.let { vm.enableBiometric(it, pw) } },
            onDisableBiometric = vm::disableBiometric,
            onArmSafPicker = vm::armSafPicker,
        )
    }
}
