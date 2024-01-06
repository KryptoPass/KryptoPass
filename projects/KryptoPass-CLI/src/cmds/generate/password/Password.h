#pragma once

#include <string>
#include <CLI/CLI.hpp>

namespace Subcommands
{
	CLI::App* GeneratePassword(
		CLI::App* cli,
		// Default Settings
		std::string& writing_system, // Soon we will have support for more writing systems, e.g. Japanese Kana.
		std::string& digits,
		std::string& specials,
		std::string& emojis,
		bool& uppercase,
		int& length
	);
}
