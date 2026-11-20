#pragma once
#include <QString>
#include <QStringList>

namespace kryptopass::desktop::native
{
// Both return the launch options as compact JSON for `launch.rs`.

// Parses this process's arguments. `--help`, `--version` and usage errors print and
// exit here, so call it after KAboutData is registered and before the single-instance
// service, which would otherwise forward them to a running instance.
QString process_command_line();

// Parses arguments a later launch forwarded through the single-instance service.
// Relative files resolve against the directory that launch ran from.
QString parse_forwarded_command_line(const QStringList &arguments, const QString &workingDirectory);
}
