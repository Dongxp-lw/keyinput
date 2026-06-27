package com.lincdkeyinput.app.ui

import uniffi.vault_core.FieldKind
import uniffi.vault_core.InputBehavior
import uniffi.vault_core.Sensitivity

/**
 * 平台侧的默认字段策略（镜像核心实现文档 §6；核心的 `default_policy` 未经 FFI 暴露，故在此复刻）。
 * 仅用于编辑器新建字段时给出合理默认值，用户可在后续版本细化。
 */
fun defaultSensitivity(kind: FieldKind): Sensitivity = when (kind) {
    FieldKind.PASSWORD, FieldKind.TOTP, FieldKind.SECRET -> Sensitivity.HIGH
    FieldKind.NOTE -> Sensitivity.SENSITIVE
    else -> Sensitivity.NORMAL
}

fun defaultInputBehavior(kind: FieldKind): InputBehavior = when (kind) {
    FieldKind.SECRET -> InputBehavior.REVEAL_ONLY
    FieldKind.NOTE, FieldKind.MULTILINE -> InputBehavior.COPY
    else -> InputBehavior.INSERT
}

/** 该字段是否默认遮蔽显示（高/敏感）。 */
fun isHidden(sensitivity: Sensitivity): Boolean = sensitivity != Sensitivity.NORMAL
