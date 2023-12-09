#include <KryptoPassLib.h>
#include <CLI/CLI.hpp>
#include <iostream>

#include "logger.h"
#include "main.h"

KryptoPass::KryptoPass(int argc, char** argv) : app{ (char*)u8"KryptoPass: Your digital fortress of secrets guarded by cryptographic wizardry! 🌟🔐", "KryptoPass" }
{
	this->argc = argc;
	this->argv = argv;

	int success = kpl::init();

	if (!success) {
		std::cerr << "Error initializing OpenSSL" << std::endl;
		std::exit(1);
	}

	app.add_option("-f,--file", filename, "A help string");
}

KryptoPass::~KryptoPass()
{
	kpl::close();
}

int KryptoPass::Parser()
{
	try {
		app.parse(argc, argv);
		return this->Handled();
	}
	catch (const CLI::ParseError& e) {
		return (app).exit(e);
	}
}

int KryptoPass::Handled()
{
	std::cout << filename << std::endl;
	return 0;
}