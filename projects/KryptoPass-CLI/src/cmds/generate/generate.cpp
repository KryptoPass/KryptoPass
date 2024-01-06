#include <string>
#include <CLI/CLI.hpp>

namespace Subcommands
{
	CLI::App* Generate(CLI::App &cli)
	{
		auto generate = cli.add_subcommand("generate", "Generate passwords");
		generate->require_subcommand(1);

		return generate;
	}
}
