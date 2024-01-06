#include <CLI/CLI.hpp>
#include <string>
#include <vector>
#include <sstream>
#include <algorithm>

#include "Password.h"

namespace Subcommands
{
	CLI::App* GeneratePassword(
		CLI::App* cli,
		// Default Settings
		std::string& writing_system,
		std::string& digits,
		std::string& specials,
		std::string& emojis,
		bool& uppercase,
		int& length
	)
	{
		auto password = cli->add_subcommand("password", "Generate passwords");
		
		// Flags
		password->add_flag("-u,--upper,--uppercase", uppercase, "Add uppercase");
		
		// Options Store Values
		password->add_option("-l,--len,--length", length, "Password length");
		password->add_option("-d,--digits", digits, "Add digits 0-9, --digits N or {min,max}");
		password->add_option("-s,--specials", specials, "Add specials, --specials N or {min,max}");
		password->add_option("-e,--emojis", emojis, "Add Unicode 2010 emojis, --specials N or {min,max}");

		return password;
	}
}
