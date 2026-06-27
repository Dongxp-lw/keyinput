package com.lincdkeyinput.app.ui

import uniffi.vault_core.EntryType
import uniffi.vault_core.FieldKind

fun entryTypeLabel(t: EntryType): String = when (t) {
    EntryType.LOGIN -> "登录"
    EntryType.SECURE_NOTE -> "安全笔记"
    EntryType.IDENTITY -> "身份"
    EntryType.PAYMENT -> "支付"
    EntryType.TEMPLATE -> "模板"
    EntryType.CUSTOM -> "自定义"
}

fun fieldKindLabel(k: FieldKind): String = when (k) {
    FieldKind.USERNAME -> "用户名"
    FieldKind.PASSWORD -> "密码"
    FieldKind.EMAIL -> "邮箱"
    FieldKind.PHONE -> "电话"
    FieldKind.TOTP -> "验证码 (TOTP)"
    FieldKind.TEXT -> "文本"
    FieldKind.MULTILINE -> "多行文本"
    FieldKind.URL -> "网址"
    FieldKind.ADDRESS -> "地址"
    FieldKind.SECRET -> "私密"
    FieldKind.NOTE -> "备注"
}
