#pragma once
#include <string>

#ifndef UNICODE_ENSURE
	#include <Windows.h>
	#define UNICODE_ENSURE SetConsoleCP(CP_UTF8);SetConsoleOutputCP(CP_UTF8)

	std::string u8ToString(const std::u8string& str)
	{
		return reinterpret_cast<const char*>(str.data());
	}

	std::u8string stringToU8(const std::string& str)
	{
		return reinterpret_cast<const char8_t*>(str.data());
	}
#endif // !UNICODE_ENSURE
