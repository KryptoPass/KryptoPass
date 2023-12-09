#pragma once

#include <windows.h>
#include <shlobj.h>

class Shell
{
private:
	std::string windowsGetFolder(REFKNOWNFOLDERID folderId, const char* errorMsg);
	REFKNOWNFOLDERID getKnownFolderId(const char* name);
public:
	std::string getPath(const char* name);
	std::string uuidv4();
};
