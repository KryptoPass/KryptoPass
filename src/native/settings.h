#pragma once
#include <KConfigGroup>
#include <KSharedConfig>

// Native preferences. They share <component name>rc (`org.kryptopass.desktoprc`)
// with the shortcuts that KActionCollection saves; every key lives here.
namespace kryptopass::desktop::native::settings
{
inline constexpr auto kSecurityGroup = "Security";
inline constexpr auto kIdleLockSeconds = "IdleLockSeconds";
inline constexpr auto kClipboardAutoClearSeconds = "ClipboardAutoClearSeconds";

inline constexpr auto kOpenPgpGroup = "OpenPGP";
// JSON owned by the Rust online-services model.
inline constexpr auto kOnlineServices = "OnlineServices";

inline KConfigGroup group(const char *name)
{
    return KSharedConfig::openConfig()->group(QLatin1String(name));
}
}
