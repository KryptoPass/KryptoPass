#pragma once

#include <QString>

namespace kryptopass::desktop::native
{

bool copy_sensitive(const QString &text, int auto_clear_seconds);
void clear_owned();
int auto_clear_seconds();
void set_auto_clear_seconds(int seconds);

} // namespace kryptopass::desktop::native
