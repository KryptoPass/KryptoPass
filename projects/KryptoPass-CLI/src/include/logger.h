#ifndef CONSOLE_LOGGER
#define CONSOLE_LOGGER

#include <string>
#include <memory>
#include <chrono>
#include <iomanip>
#include <iostream>
#include <spdlog/spdlog.h>
#include <spdlog/sinks/stdout_color_sinks.h>

class Console {
private:
	std::shared_ptr<spdlog::logger> console;
	time_t timestamp{ std::chrono::system_clock::to_time_t(std::chrono::system_clock::now()) };

public:
	Console(spdlog::level::level_enum level = spdlog::level::trace)
	{
		console = spdlog::get("console");
		if (!console) {
			console = spdlog::stdout_color_mt("console");
		}

		console->set_pattern("%^[%o] [%l] [%d/%m/%Y %H:%M:%S.%e %p UTC %z] [%v]%$");
		console->set_level(level);
	}

	template <typename... Args>
	void log(spdlog::format_string_t<Args...> fmt, Args&&... args) {
		console->trace(fmt, std::forward<Args>(args)...);
	}

	template <typename... Args>
	void debug(spdlog::format_string_t<Args...> fmt, Args&&... args) {
		console->debug(fmt, std::forward<Args>(args)...);
	}

	template <typename... Args>
	void info(spdlog::format_string_t<Args...> fmt, Args&&... args) {
		console->info(fmt, std::forward<Args>(args)...);
	}

	template <typename... Args>
	void warn(spdlog::format_string_t<Args...> fmt, Args&&... args) {
		console->warn(fmt, std::forward<Args>(args)...);
	}

	template <typename... Args>
	void error(spdlog::format_string_t<Args...> fmt, Args&&... args) {
		console->error(fmt, std::forward<Args>(args)...);
	}

	template <typename... Args>
	void critical(spdlog::format_string_t<Args...> fmt, Args&&... args) {
		console->critical(fmt, std::forward<Args>(args)...);
	}
};

#endif // CONSOLE_LOGGER
