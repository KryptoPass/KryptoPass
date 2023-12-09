#include <openssl/crypto.h>
#include "KryptoPassLib.h"
#include <iostream>

namespace kpl {

	void PrintHelloWorld()
	{
		std::cout << "Hello World!\n";
		std::cin.get();
	}

	int init()
	{
		return OPENSSL_init_crypto(OPENSSL_INIT_LOAD_CONFIG, NULL);
	}

	void close()
	{

	}
}