from espn_fetcher import get_league_data, update_data_files
from grok_integration import query_grok
import yaml

def load_settings():
    with open('config.yaml', 'r') as f:
        return yaml.safe_load(f)['settings']

def main():
    # Update data files
    update_data_files()
    
    settings = load_settings()
    rosters = get_league_data()
    
    # Test Grok Integration: Waiver Pickup Suggestion
    print("Testing Grok Integration - Waiver Suggestion...")
    waiver_suggestion = query_grok("waiver", rosters)
    print(f"Waiver Suggestions: {waiver_suggestion}\n")
    
    # Test Grok Integration: Lineup Recommendation
    print("Testing Grok Integration - Lineup Recommendation...")
    lineup_suggestion = query_grok("lineup", rosters)
    print(f"Lineup Recommendations: {lineup_suggestion}\n")
    
    # Test Grok Integration: Trade Analysis
    trade_data = {
        "my_team": "Waddle We Do?",  # Replace with your team
        "offer": ["PlayerToOffer"],  # Replace with real players
        "receive": ["PlayerToReceive"]
    }
    print("Testing Grok Integration - Trade Analysis...")
    trade_suggestion = query_grok("trade", trade_data)
    print(f"Trade Analysis: {trade_suggestion}")

if __name__ == "__main__":
    main()