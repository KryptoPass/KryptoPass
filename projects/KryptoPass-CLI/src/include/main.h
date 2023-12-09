#pragma once

#include <CLI/CLI.hpp>
#include "exceptions/kpexcept.h"

class KryptoPass
{
public:
	KryptoPass(int argc, char** argv);
	~KryptoPass();
	int Parser();

private:
	int Handled();
	int argc;
	char** argv;
	CLI::App app;

	// Subcommands
	std::string filename = "default";
};