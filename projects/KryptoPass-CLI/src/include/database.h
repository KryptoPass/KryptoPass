#pragma once

#include <SQLiteCpp/SQLiteCpp.h>
#include "exceptions/dbexcept.h"
#include <zip.h>

#include "logger.h"
#include "shell.h"

class Database
{
public:
	Database();
	~Database();

private:
	void create(Shell& shell, Console& console, const char* path);
	std::string createSQLiteDB(Shell& shell, zip_t* zip);
	void addDir(zip_t* zip, const std::string& dirName);
	void initializeFolders(zip_t* zip);

	zip_t* zip_;
};
