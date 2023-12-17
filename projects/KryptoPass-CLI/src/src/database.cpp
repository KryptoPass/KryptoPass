#include <SQLiteCpp/SQLiteCpp.h>
#include <filesystem>
#include <iostream>
#include <string>
#include <zip.h>

#include "database.h"
#include "logger.h"
#include "shell.h"

const char* EXT_DATABASE_NAME = "Database.kpdb";
const char* INT_DATABASE_NAME = "KryptoPass.db3";

Database::Database()
{
	Shell shell;		// Init Shell Utils
	Console console;	// Init logger

	// Get home app path
	auto homeDir = std::filesystem::path(shell.getPath("roaming")) / "KryptoPass";
	auto kpdb = homeDir / EXT_DATABASE_NAME;	// Database name

	// if (!exists(home | kpdb)) create();
	if (!std::filesystem::exists(kpdb)) {
		if (!std::filesystem::exists(homeDir)) {
			console.log("{}: does not exist, will be created", homeDir.string());
			std::filesystem::create_directories(homeDir);
		}

		create(shell, console, kpdb.string().c_str());
	}
	else {
		int openerr;
		zip_ = zip_open(kpdb.string().c_str(), 0, &openerr);

		if (!zip_) {
			zip_error_t error;
			zip_error_init_with_code(&error, openerr);
			console.error("{}: cannot open zip archive '{}': {}", kpdb.string(), EXT_DATABASE_NAME, zip_error_strerror(&error));
			zip_error_fini(&error);
			throw DBException("Database open error");
		}
	}
}

Database::~Database()
{
	if (zip_) {
		zip_close(zip_);
	}
}

void Database::create(Shell& shell, Console& console, const char* path)
{
	int openerr;
	zip_ = zip_open(path, ZIP_CREATE | ZIP_TRUNCATE, &openerr);

	if (!zip_) {
		zip_error_t error;
		zip_error_init_with_code(&error, openerr);
		console.error("{}: failed to create the database {}: {}", path, EXT_DATABASE_NAME, zip_error_strerror(&error));
		zip_error_fini(&error);
		throw DBException("Database create error");
	}

	try {
		// Inicializa las carpetas
		initializeFolders(zip_);

		// Crear base de datos SQLite y agregar al archivo ZIP
		auto tmpdb = createSQLiteDB(shell, zip_);
		
		zip_source* source = zip_source_file(zip_, tmpdb.c_str(), 0, 0);
		if (source == nullptr) {
			throw std::runtime_error("Error creating zip source");
		}

		zip_file_add(zip_, INT_DATABASE_NAME, source, ZIP_FL_ENC_UTF_8 | ZIP_FL_OVERWRITE);
		//std::filesystem::remove(tmpdb);
	}
	catch (const std::runtime_error& e) {
		console.error("Runtime error: {}", e.what());
		throw; // Re-lanza la excepción para manejarla en el nivel superior
	}
	catch (const std::exception& e) {
		console.error("Standard exception: {}", e.what());
		throw;
	}
}

void Database::initializeFolders(zip_t* zip)
{
	// Auditing Folders
	addDir(zip, "Auditing");
	addDir(zip, "Auditing/Change History");
	addDir(zip, "Auditing/Version Management");

	// Content Folders
	addDir(zip, "Content Folders");
	addDir(zip, "Content Folders/Audios");
	addDir(zip, "Content Folders/Binaries");
	addDir(zip, "Content Folders/Documents");
	addDir(zip, "Content Folders/Images");
	addDir(zip, "Content Folders/Notes");
	addDir(zip, "Content Folders/Others");
	addDir(zip, "Content Folders/Videos");

	// Metadata Folders
	addDir(zip, "Metadata");
	addDir(zip, "User Assets");
	addDir(zip, "User Interface");
}

void Database::addDir(zip_t* zip, const std::string& dirName)
{
	if (zip_dir_add(zip, dirName.c_str(), ZIP_FL_ENC_UTF_8) < 0) {
		throw std::runtime_error("Error adding directory to zip archive");
	}
}

std::string Database::createSQLiteDB(Shell& shell, zip_t* zip)
{
	auto kpdb = std::filesystem::path(shell.getPath("temp")) / "ARCHIVO TEMPORAL GENIAL";
	auto kpdbstd = kpdb.string();

	SQLite::Database db(kpdbstd, SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);

	return kpdbstd;
}
