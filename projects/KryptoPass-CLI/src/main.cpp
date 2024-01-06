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
#define KRYPTOPASS_VERSION "1.0.0"

#include <stdlib.h>
#include <KPL.h>

#include "core/Application.h"
#include "core/Console.h"

#include <Windows.h>

int main(int argc, char** argv)
{
	SetConsoleCP(CP_UTF8);
	SetConsoleOutputCP(CP_UTF8);

	const char8_t* hiraganaaa = u8"平仮名✨🥴🌟🔐";
	std::cout << reinterpret_cast<const char*>(hiraganaaa) << std::endl;

	//UNICODE_ENSURE;
	Console console;

	Application app(argc, argv, console);
	app.setApplicationName("KryptoPass", (char*)u8"KryptoPass: Your digital fortress of secrets guarded by cryptographic wizardry! 🌟🔐");
	app.setApplicationVersion(KRYPTOPASS_VERSION);

	const char8_t* hiraganaa = u8"平仮名✨🥴🌟🔐";
	std::cout << reinterpret_cast<const char*>(hiraganaa) << std::endl;

	int code = app.commandLineParser();
	if (code != 0) {
		return code;
	}

	const char8_t* hiragana = u8"平仮名✨🥴🌟🔐";
	std::cout << reinterpret_cast<const char*>(hiragana) << std::endl;

	if (!KPL::init()) {
		console.critical("Fatal error while testing the cryptographic functions.\n {}", KPL::errorString());

		return EXIT_FAILURE;
	}

	app.bootstrap();

	return EXIT_SUCCESS;
}
