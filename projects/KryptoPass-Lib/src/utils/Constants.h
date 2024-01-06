#pragma once

#include <array>

namespace Constants
{
	extern std::array<char8_t, 10> digits;
	extern std::array<char8_t, 32> punctuation;
	extern std::array<char8_t, 26> latinAlphabetLower;
	extern std::array<char8_t, 26> latinAlphabetUpper;
	extern std::array<char8_t, 71> hiragana;
	extern std::array<char8_t, 71> katakana;
}

//std::vector<int> selected_characters;

//// Añade los conjuntos de caracteres seleccionados por el usuario
//if (user_selected_digits) {
//	selected_characters.insert(selected_characters.end(), digits.begin(), digits.end());
//}
//if (user_selected_ascii_lowercase) {
//	selected_characters.insert(selected_characters.end(), ascii_lowercase.begin(), ascii_lowercase.end());
//}
//// Haz lo mismo para los otros conjuntos de caracteres...

//const int length = 10;
//unsigned char buffer[length];
//RAND_bytes(buffer, length);

//std::string password;
//for (int i = 0; i < length; i++) {
//	password += selected_characters[buffer[i] % selected_characters.size()];
//}

//return password;


//std::array<int, 236> all_characters;
//all_characters[0:10] = digits;
//all_characters[10:36] = ascii_lowercase;
//all_characters[36:62] = ascii_uppercase;
//all_characters[62:94] = punctuation;
//all_characters[94:165] = hiragana;
//all_characters[165:236] = katakana;
//
//const int length = 10;
//unsigned char buffer[length];
//RAND_bytes(buffer, length);
//
//std::string password;
//for (int i = 0; i < length; i++) {
//	password += all_characters[buffer[i] % all_characters.size()];
//}
//
//return password;



//#include <array>
//#include <string>
//#include <random>
//#include <openssl/rand.h>
//
//std::string generate_random_number() {
//    std::array<char, 10> digits{ '0','1','2','3','4','5','6','7','8','9' };
//    std::array<int, 2> number{ 1, 4 };
//
//    unsigned char buffer[1];
//    if (RAND_bytes(buffer, sizeof(buffer)) != 1) {
//        // Error handling
//    }
//
//    int random_number = number[0] + (buffer[0] % (number[1] - number[0] + 1));
//    std::string result;
//
//    std::random_device rd;
//    std::mt19937 gen(rd());
//    std::uniform_int_distribution<> dis(0, 9);
//
//    for (int i = 0; i < random_number; ++i) {
//        result += digits[dis(gen)];
//    }
//
//    return result;
//}
