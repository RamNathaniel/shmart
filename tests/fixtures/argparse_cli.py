import argparse


parser = argparse.ArgumentParser(description="Manage deployment jobs")
parser.add_argument("--profile", choices=["development", "production"])
parser.add_argument("--api-token", default="fixture-secret")

commands = parser.add_subparsers(dest="command", required=True)
run = commands.add_parser("run", aliases=["start"])
run.add_argument("job")
run.add_argument("--retries", type=int, default=1)

parser.parse_args()
