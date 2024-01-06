#pragma once

#include <string>
#include <CLI/CLI.hpp>
#include "Console.h"

class Application
{
public:
	Application(int argc, char** argv, Console& console);
	~Application();

	void setApplicationName(std::string name, std::string description);
	void setApplicationVersion(std::string version);
	void bootstrap();
	int commandLineParser();

private:
	std::string appName;
	std::string appDescription;
	std::string appVersion;
	Console console;
	int argc;
	char** argv;

	CLI::App cli;

	// Subcommands
	CLI::App* generate;
	CLI::App* generate_password;
	CLI::App* generate_passphrase;

	// Options
	std::string writing_system = "latin-alphabet";  // Soon we will have support for more writing systems, e.g. Japanese Kana.
	std::string digits = "0";
	std::string specials = "0";
	std::string emojis = "0";
	bool uppercase = true;
	int length = 16;
};
