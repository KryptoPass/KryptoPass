#include <filesystem>
#include <stdexcept>
#include <windows.h>
#include <iostream>
#include <shlobj.h>
#include <sstream>
#include <iomanip>
#include <string>
#include <random>

#include "shell.h"

std::string Shell::windowsGetFolder(REFKNOWNFOLDERID folderId, const char* errorMsg)
{
	PWSTR path;
	HRESULT hr = SHGetKnownFolderPath(folderId, KF_FLAG_CREATE, NULL, &path);

	if (!SUCCEEDED(hr)) {
		throw std::runtime_error(errorMsg);
	}

	std::filesystem::path p(path);
	CoTaskMemFree(path);

	return p.string();
}

REFKNOWNFOLDERID Shell::getKnownFolderId(const char* name) {
	if (strcmp(name, "home") == 0) return FOLDERID_Profile;
	if (strcmp(name, "appdata") == 0) return FOLDERID_Profile;  // Ahora apunta a la carpeta de perfil del usuario
	if (strcmp(name, "temp") == 0) return FOLDERID_LocalAppData;
	if (strcmp(name, "desktop") == 0) return FOLDERID_Desktop;
	if (strcmp(name, "documents") == 0) return FOLDERID_Documents;
	if (strcmp(name, "downloads") == 0) return FOLDERID_Downloads;
	if (strcmp(name, "music") == 0) return FOLDERID_Music;
	if (strcmp(name, "pictures") == 0) return FOLDERID_Pictures;
	if (strcmp(name, "videos") == 0) return FOLDERID_Videos;
	if (strcmp(name, "roaming") == 0) return FOLDERID_RoamingAppData;
	if (strcmp(name, "local") == 0) return FOLDERID_LocalAppData;


	throw std::invalid_argument("Unknown folder name");
}

std::string Shell::getPath(const char* name) {
	try
	{
		REFKNOWNFOLDERID folderId = getKnownFolderId(name);
		std::string path = windowsGetFolder(folderId, "Failed to get folder path");

		if (strcmp(name, "appdata") == 0) {
			return (std::filesystem::path(path) / "AppData").string();  // Agrega \AppData al final
		}
		else if (strcmp(name, "temp") == 0) {
			return (std::filesystem::path(path) / "Temp").string();  // Agrega \Temp al final
		}

		return path;
	}
	catch (const std::exception&)
	{
		return "";  // Retorna una cadena vacía en lugar de nullptr
	}
}

// Función para generar un UUID v4
std::string Shell::uuidv4() {
	std::random_device rd; // Generador de números aleatorios
	std::mt19937 gen(rd()); // Motor de números aleatorios basado en Mersenne Twister
	std::uniform_int_distribution<> dis(0, 15); // Distribución para números de 0 a 15 (hexadecimales)
	std::uniform_int_distribution<> dis2(8, 11); // Distribución para la variante del UUID (8 a 11)

	std::stringstream ss; // Stream para construir el string del UUID
	ss << std::hex; // Configurar el stream para manejar números hexadecimales

	// Generar la primera parte del UUID (8 dígitos hexadecimales)
	for (int i = 0; i < 8; i++) {
		ss << dis(gen); // Ejemplo de salida: "9af7f46e"
	}
	ss << "-";

	// Segunda parte del UUID (4 dígitos hexadecimales)
	for (int i = 0; i < 4; i++) {
		ss << dis(gen); // Ejemplo: "e1d9"
	}
	ss << "-";

	// Tercera parte del UUID (comienza con "4" seguido de 3 dígitos hexadecimales)
	ss << "4"; // Indica la versión 4 del UUID
	for (int i = 0; i < 3; i++) {
		ss << dis(gen); // Ejemplo: "4b8c3"
	}
	ss << "-";

	// Cuarta parte del UUID (variante 8, 9, a, o b seguido de 3 dígitos hexadecimales)
	ss << dis2(gen); // Ejemplo: "a"
	for (int i = 0; i < 3; i++) {
		ss << dis(gen); // Ejemplo: "a9f4d"
	}
	ss << "-";

	// Quinta parte del UUID (12 dígitos hexadecimales)
	for (int i = 0; i < 12; i++) {
		ss << dis(gen); // Ejemplo: "6f921d24e9eb"
	}

	return ss.str(); // Devuelve el UUID completo como string
}