#pragma once
#include "rust/cxx.h"
#include <QQmlEngine>
#include <QString>
#include <QStringList>
namespace kryptopass::desktop::native
{
rust::Vec<std::uint8_t> normalize_entry_icon(rust::Slice<const std::uint8_t> bytes);
bool valid_entry_icon(rust::Slice<const std::uint8_t> bytes);
void install_entry_icon_provider(QQmlEngine &engine, const QString &appId);
void prefetch_favicons(const QStringList &hosts);
rust::Vec<std::uint8_t> entry_icon_bytes(rust::Str source) noexcept;
}
