/*
* This file is part of the KryptoPass distribution https://github.com/KryptoPass/KryptoPass.
* Copyright (C) 2023  Gabriel Maizo <maizogabriel@proton.me>
*
* This program is free software: you can redistribute it and/or modify
* it under the terms of the GNU General Public License as published by
* the Free Software Foundation, either version 3 of the License, or
* (at your option) any later version.
*
* This program is distributed in the hope that it will be useful,
* but WITHOUT ANY WARRANTY; without even the implied warranty of
* MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
* GNU General Public License for more details.
*
* You should have received a copy of the GNU General Public License
* along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

#include <KPL.h>
#include <CLI/CLI.hpp>
#include <locale>
#include <codecvt>
#include <string>

#include "Console.h"
#include "Config.h"
#include "Application.h"

// Utils
#include "utils/Shell.h"
#include "utils/Parser.h"
#include "utils/Unicode.h"

// Commands
#include "cmds/SubCommands.h"

Application::Application(int argc, char** argv, Console& console): cli()
{
	this->argc = argc;
	this->argv = argv;
	this->console = console;

	// Register subcommands
	generate = Subcommands::Generate(cli);
	generate_password = Subcommands::GeneratePassword(generate, writing_system, digits, specials, emojis, uppercase, length);
	generate_passphrase = Subcommands::GeneratePassphrase(generate);
}

Application::~Application()
{
	KPL::close();
}

void Application::setApplicationName(std::string name, std::string description)
{
	this->appName = name;
	this->appDescription = name;
	cli.name(name);
	cli.description(description);
}

void Application::setApplicationVersion(std::string version)
{
	this->appVersion = version;
	cli.set_version_flag("--version", this->appName + ": " + version);
}

int Application::commandLineParser()
{
	try {
		cli.parse(argc, argv);

		if (cli.get_subcommands().size() == 0) {
			std::cout << cli.help();
			return 1;
		}

		return 0;
	}
	catch (CLI::ParseError& e) {
		int exitCode = e.get_exit_code();
		
		// A subcommand is required or no arguments were provided.
		if (exitCode == 106 || exitCode != 0) {
			std::cout << cli.help();
		} else {
			console.error("Parse Error: {} Exit Code {}", e.what(), exitCode);
		}

		return exitCode;
	}
	catch (const std::exception& e) {
		console.error("Error: {}", e.what());
		return 1;
	}
}

void Application::bootstrap()
{
	if (generate->parsed()) {
		if (generate_password->parsed()) {
			const char8_t* randPassword = KPL::generator::password();

			const char8_t* hiragana = u8"平仮名✨🥴🌟🔐";
			std::cout << reinterpret_cast<const char*>(hiragana) << std::endl;

			std::cout << reinterpret_cast<const char*>(randPassword) << std::endl;
		}

		if (generate_passphrase->parsed()) {
			console.log("Generate passphrase");
			//KPL::generator::passphrase(10);
		}
	}
}

// std::string randPassword = KPL::generator::password(length, digits, emojis, specials, uppercase, writing_system);
