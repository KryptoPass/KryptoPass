// SPDX-FileCopyrightText: 2024 Darshan Phaldesai <dev.darshanphaldesai@gmail.com>
// SPDX-License-Identifier: MPL-2.0

#include "cxx-kde-frameworks/klocalizedstring.h"

namespace rust {
namespace kf6 {

void setApplicationDomain(const QByteArray &domain) {
  KLocalizedString::setApplicationDomain(domain);
}

auto applicationDomain() -> QByteArray {
  return KLocalizedString::applicationDomain();
}

void addDomainLocaleDir(const QByteArray &domain, const QString &path) {
  KLocalizedString::addDomainLocaleDir(domain, path);
}

auto r_subs_int(const KLocalizedString &string, std::int64_t argument)
    -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      string.subs(static_cast<qlonglong>(argument)));
}

auto r_subs_uint(const KLocalizedString &string, std::uint64_t argument)
    -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      string.subs(static_cast<qulonglong>(argument)));
}

auto r_subs_double(const KLocalizedString &string, double argument)
    -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(string.subs(argument));
}

auto r_subs_string(const KLocalizedString &string, const QString &argument)
    -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(string.subs(argument));
}

auto r_ki18n(rust::String text) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(ki18n(text.c_str()));
}

auto r_ki18nc(rust::String context,
              rust::String text) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      ki18nc(context.c_str(), text.c_str()));
}
auto r_ki18ncp(rust::String context, rust::String singular,
               rust::String plural) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      ki18ncp(context.c_str(), singular.c_str(), plural.c_str()));
}
auto r_ki18nd(rust::String domain,
              rust::String text) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      ki18nd(domain.c_str(), text.c_str()));
}
auto r_ki18ndc(rust::String domain, rust::String context,
               rust::String text) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      ki18ndc(domain.c_str(), context.c_str(), text.c_str()));
}
auto r_ki18ndcp(rust::String domain, rust::String context,
                rust::String singular,
                rust::String plural) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(ki18ndcp(
      domain.c_str(), context.c_str(), singular.c_str(), plural.c_str()));
}
auto r_ki18ndp(rust::String domain, rust::String singular,
               rust::String plural) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      ki18ndp(domain.c_str(), singular.c_str(), plural.c_str()));
}
auto r_ki18np(rust::String singular,
              rust::String plural) -> std::unique_ptr<KLocalizedString> {
  return std::make_unique<KLocalizedString>(
      ki18np(singular.c_str(), plural.c_str()));
}

} // namespace kf6
} // namespace rust
