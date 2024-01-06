#include <array>
#include <string>
#include <sstream>
#include <iostream>

std::array<int, 2> getRangeFromString(std::string number_str)
{
	std::array range{ 16, 16 };

	std::stringstream ss(number_str);

	char open_bracket, close_bracket, comma;
	int num1, num2;

	if (number_str.front() == '[' && number_str.back() == ']') {
		ss >> open_bracket >> num1 >> comma >> num2 >> close_bracket;
		if (ss.fail() || open_bracket != '[' || close_bracket != ']' || comma != ',') {
			std::cerr << "Error: Invalid range format.\n";
		}

		if (num1 > num2) {
			std::cerr << "Error: The first number in the range should not be greater than the second number.\n";
		}

		range[0] = num1;
		range[1] = num2;
	}
	else {
		ss >> num1;

		if (ss.fail()) {
			std::cerr << "Error: Input is not a number.\n";
		}

		// [Min, Max]
		range[0] = num1;
		range[1] = num1;
	}

	return range;
}
