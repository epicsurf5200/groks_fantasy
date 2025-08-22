from espn_fetcher import get_league_data
from grok_integration import query_grok

def main():
    rosters = get_league_data()
    suggestion = query_grok("Suggest waiver pickups based on these rosters.", rosters)
    print(f"Waiver Suggestions: {suggestion}")

if __name__ == "__main__":
    main()