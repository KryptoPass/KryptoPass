#include "kryptopass_app/src/demo/startup.cxx.h"

int main(int argc, char *argv[])
{
    rust::Vec<rust::String> args;
    for (int i = 0; i < argc; ++i)
        args.push_back(rust::String(argv[i]));
    return kryptopass::run(std::move(args));
}
