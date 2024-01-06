#pragma once

#include <string>

namespace KPL {
	int init();
	void close();
	std::string errorString();
	
	namespace generator
	{
		//std::string password(std::array<int, 2> len, std::array<int, 2> dig, std::array<int, 2> emo, std::array<int, 2> spe, bool upp, std::string ws);
		const char8_t* password();
	};
}
