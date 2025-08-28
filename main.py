from espn_fetcher import get_league_data, update_data_files
from grok_integration import query_grok
import yaml

def load_settings():
    with open('config.yaml', 'r') as f:
        return yaml.safe_load(f)['settings']

def main():
    # Update data files with latest ESPN info
    update_data_files()

    settings = load_settings()
    rosters = get_league_data()
    prompt = f"Suggest waiver pickups for a {settings['scoring_type']} league."
    suggestion = query_grok(prompt, rosters)
    print(f"Waiver Suggestions: {suggestion}")

if __name__ == "__main__":
    main()