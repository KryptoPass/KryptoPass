#include <CLI/CLI.hpp>
#include <string>

namespace Subcommands
{
	CLI::App* GeneratePassphrase(CLI::App* cli)
	{
		auto passphrase = cli->add_subcommand("passphrase", "Generate passwords");

		return passphrase;
	}
}
