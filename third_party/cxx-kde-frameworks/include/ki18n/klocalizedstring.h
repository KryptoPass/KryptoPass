// SPDX-FileCopyrightText: 2024 Darshan Phaldesai <dev.darshanphaldesai@gmail.com>
// SPDX-License-Identifier: MPL-2.0

#pragma once

#include <KLocalizedString>

#include "rust/cxx.h"

namespace rust {
namespace kf6 {
void setApplicationDomain(const QByteArray &domain);

auto applicationDomain() -> QByteArray;

void addDomainLocaleDir(const QByteArray &domain, const QString &path);

auto r_subs_int(const KLocalizedString &string, std::int64_t argument)
    -> std::unique_ptr<KLocalizedString>;
auto r_subs_uint(const KLocalizedString &string, std::uint64_t argument)
    -> std::unique_ptr<KLocalizedString>;
auto r_subs_double(const KLocalizedString &string, double argument)
    -> std::unique_ptr<KLocalizedString>;
auto r_subs_string(const KLocalizedString &string, const QString &argument)
    -> std::unique_ptr<KLocalizedString>;

auto r_ki18n(rust::String text) -> std::unique_ptr<KLocalizedString>;
auto r_ki18nc(rust::String context, rust::String text)
    -> std::unique_ptr<KLocalizedString>;
auto r_ki18ncp(rust::String context, rust::String singular, rust::String plural)
    -> std::unique_ptr<KLocalizedString>;
auto r_ki18nd(rust::String domain, rust::String text)
    -> std::unique_ptr<KLocalizedString>;
auto r_ki18ndc(rust::String domain, rust::String context, rust::String text)
    -> std::unique_ptr<KLocalizedString>;
auto r_ki18ndcp(rust::String domain, rust::String context,
                rust::String singular, rust::String plural)
    -> std::unique_ptr<KLocalizedString>;
auto r_ki18ndp(rust::String domain, rust::String singular, rust::String plural)
    -> std::unique_ptr<KLocalizedString>;
auto r_ki18np(rust::String singular, rust::String plural)
    -> std::unique_ptr<KLocalizedString>;

} // namespace kf6
} // namespace rust
