#include <openssl/crypto.h>
#include <openssl/rand.h>
#include <iostream>
#include <string>
#include <vector>
#include <array>

#include "utils/Constants.h"
#include "KPL.h"

template <typename T, std::size_t N>
std::u8string generate_random_string(const std::array<T, N>& arr, int min, int max) {
	unsigned char buffer[1];
	do {
		if (!RAND_bytes(buffer, sizeof(buffer))) {
			throw std::runtime_error("RAND_bytes failed");
		}
	} while (buffer[0] < min || buffer[0] > max);  // rejection sampling for length
	int length = buffer[0];

	std::u8string result;

	for (int i = 0; i < length; ++i) {
		do {
			if (!RAND_bytes(buffer, sizeof(buffer))) {
				throw std::runtime_error("RAND_bytes failed");
			}
		} while (buffer[0] >= N * (256 / N));  // rejection sampling for index
		int index = buffer[0] % N;
		result += arr[index];
	}

	return result;
}


//template <typename T, std::size_t N>
//std::string generate_random_string(const std::array<T, N>& arr, int min, int max) {
//	std::string result;
//	unsigned char buffer[1];
//
//	for (int i = 0; i < max; ++i) {
//		do {
//			if (!RAND_bytes(buffer, sizeof(buffer))) {
//				throw std::runtime_error("RAND_bytes failed");
//			}
//		} while (buffer[0] >= N * (256 / N));  // rejection sampling
//		int index = buffer[0] % N;
//		result += arr[index];
//		if (i >= min) {
//			if (!RAND_bytes(buffer, sizeof(buffer))) {
//				throw std::runtime_error("RAND_bytes failed");
//			}
//			if (buffer[0] % 2) {
//				break;
//			}
//		}
//	}
//
//	return result;
//}

namespace KPL {
	int init()
	{
		return OPENSSL_init_crypto(OPENSSL_INIT_LOAD_CONFIG, NULL);
	}

	void close() {}
	std::string errorString() { return ""; }

	namespace generator
	{

		//std::string password(std::array<int, 2> len, std::array<int, 2> dig, std::array<int, 2> emo, std::array<int, 2> spe, bool upp, std::string ws)
		const char8_t* password()
		{
			std::array<char, 10> digits{ '0','1','2','3','4','5','6','7','8','9' };
			std::array<int, 2> number{ 10, 10 };

			//for (int i = 0; i <= 40; ++i) {
				//std::cout << "Random string: " << generate_random_string(Constants::katakana, number[0], number[1]) << std::endl;
			//}
			return u8"'さ', 'し', 'す', 'せ', 'そ'";
			//return generate_random_string(Constants::katakana, number[0], number[1]);
		}
	};
}

//Random string :
//Random string :
//Random string : 5
//Random string : 3
//Random string : 9
//Random string : 4
//Random string : 9
//Random string :
//Random string :
//Random string : 1
//Random string :
//Random string :
//Random string :
//Random string : 6
//Random string :
//Random string : 7
//Random string :
//Random string :
//Random string :
//Random string :
//Random string :
//Random string :
//Random string : 8
//Random string :
//Random string :
//Random string :
//Random string :
//Random string : 1
//Random string :
//Random string : 9
//Random string : 2
//Random string :
//Random string :
//Random string : 2
//Random string :
//Random string : 5
//Random string : 4
//Random string :
//Random string :
//Random string : 1
//Random string : 2

//Random string : 1602838306
//Random string : 4866
//Random string : 5771806581
//Random string : 171651340
//Random string : 58468
//Random string : 10406
//Random string :
//Random string : 697675
//Random string : 739274
//Random string : 50840
//Random string : 6
//Random string : 5117902537
//Random string : 6
//Random string : 86426
//Random string : 093
//Random string : 61807536
//Random string : 07570729
//Random string : 626834
//Random string : 4382
//Random string :
//Random string : 6468349
//Random string : 23730
//Random string : 35846
//Random string : 9725
//Random string : 4407730
//Random string : 3484
//Random string : 99
//Random string : 08513733
//Random string : 9944999
//Random string : 82
//Random string : 45322222
//Random string : 7281930
//Random string : 785
//Random string :
//Random string : 6499
//Random string : 62
//Random string : 502088082
//Random string : 981156
//Random string : 69373148
//Random string : 9649990857
//Random string : 5

//template <typename T, std::size_t N>
//std::string generate_random_string(const std::array<T, N>& arr, int min, int max) {
//	std::string result;
//	unsigned char buffer[1];
//	for (int i = 0; i < max; ++i) {
//		if (RAND_bytes(buffer, sizeof(buffer))) {
//			int index = buffer[0] % N;
//			result += arr[index];
//		}
//		if (i >= min && rand() % 2) {
//			break;
//		}
//	}
//	return result;
//}

//kryptopass generate password --length 10 --writing-system latin-alphabet japanese-kana chinese-writing
//auto kana = password->add_option("-k,--kana", length, "The use of the Japanese kana will replace the use of the Latin alphabet");
//upper->excludes(kana);
//kana->excludes(upper);
//!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~
			//std::vector<std::string> hiragana = {
			//	"あ", "い", "う", "え", "お",
			//	"か", "き", "く", "け", "こ",
			//	"さ", "し", "す", "せ", "そ",
			//	"た", "ち", "つ", "て", "と",
			//	"な", "に", "ぬ", "ね", "の",
			//	"は", "ひ", "ふ", "へ", "ほ",
			//	"ま", "み", "む", "め", "も",
			//	"や", "ゆ", "よ",
			//	"ら", "り", "る", "れ", "ろ",
			//	"わ", "を", "ん" };


			//int minLength = len[0];
			//int maxLength = len[1];

			//int minDigits = dig[0];
			//int maxDigits = dig[1];

			//int minEmojis = emo[0];
			//int maxEmojis = emo[1];

			//int minPunctuation = spe[0];
			//int maxPunctuation = spe[1];

			//if ((minDigits + minEmojis + minPunctuation) > maxLength) {
			//	// Error
			//}

			//std::vector<char> writingSystem;

			//if (ws == "latin-alphabet") {
			//	writingSystem.assign(Constants::latinAlphabetLower.begin(), Constants::latinAlphabetLower.end());
			//	writingSystem.insert(writingSystem.end(), Constants::latinAlphabetUpper.begin(), Constants::latinAlphabetUpper.end());
			//}
			//else if (ws == "japanese-kana") {
			//	writingSystem.assign(Constants::hiragana.begin(), Constants::hiragana.end());
			//	writingSystem.insert(writingSystem.end(), Constants::katakana.begin(), Constants::katakana.end());
			//}

			//std::vector<char> digits;
			//unsigned char buffer[maxDigits];

			//std::vector<int> digitsRng;

			//for (int i = minDigits; i <= maxDigits; ++i) {
			//	digitsRng.push_back(i);
			//}


			//for (int i = 0; i < maxDigits; ++i) {
			//	RAND_bytes(buffer, maxDigits);
			//	password += Constants::digits[buffer[i] % Constants::digits.size()];
			//}

			//for (int i = 0; i < length; i++) {
				//password += Constants::katakana[buffer[i] % Constants::katakana.size()];
			//}
			//std::string password;
			//return password;
