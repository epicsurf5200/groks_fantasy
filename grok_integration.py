import requests
import yaml
import json
import os
from espn_fetcher import get_league

def load_grok_config():
    with open('config.yaml', 'r') as f:
        return yaml.safe_load(f)['grok']

def load_yaml_file(file_path):
    full_path = os.path.join('league_data', file_path)
    if not os.path.exists(full_path):
        return None
    try:
        with open(full_path, 'r') as f:
            return yaml.safe_load(f)
    except yaml.YAMLError as e:
        print(f"Error loading {full_path}: {e}")
        return None

def build_fantasy_prompt(query_type, data=None, include_league_data=True):
    """Builds a prompt tailored for fantasy football queries."""
    settings = yaml.safe_load(open('config.yaml'))['settings']
    prompt = f"You are an expert in fantasy football for a {settings['scoring_type']} league. "

    if query_type == "trade":
        prompt += "Analyze the following trade proposal and suggest if it's fair or beneficial. Provide reasoning based on player performance, team needs, and league context."
    elif query_type == "waiver":
        prompt += "Suggest the top 3 waiver wire pickups based on recent performance, team needs, and league context. Include reasoning."
    elif query_type == "lineup":
        prompt += "Recommend an optimal starting lineup for this week based on player performance, matchups, and league context."
    else:
        prompt += query_type  # Custom prompt

    if include_league_data:
        league = get_league()
        previous_week = max(league.current_week - 1, 1)
        
        activity_data = load_yaml_file('league_activity.yaml') or {}
        standings_data = load_yaml_file('league_standings.yaml') or {}
        current_week_data = load_yaml_file('league_current_week.yaml') or {}
        week_result_data = load_yaml_file(f'league_week{previous_week}_result.yaml') or {}

        prompt += "\n\nLeague Context:"
        prompt += f"\nRecent Trades/Acquisitions: {json.dumps(activity_data, indent=2)}"
        prompt += f"\nStandings (Points/Expected Finish): {json.dumps(standings_data, indent=2)}"
        prompt += f"\nCurrent Week Matchups (Projected Points): {json.dumps(current_week_data, indent=2)}"
        prompt += f"\nPrevious Week Results (Player Points): {json.dumps(week_result_data, indent=2)}"

    if data:
        prompt += f"\nAdditional Data (e.g., rosters, trade details): {json.dumps(data, indent=2)}"

    return prompt

def query_grok(query_type, data=None, include_league_data=True):
    config = load_grok_config()
    prompt = build_fantasy_prompt(query_type, data, include_league_data)
    
    url = "https://api.x.ai/v1/chat/completions"
    headers = {
        "Authorization": f"Bearer {config['api_key']}",
        "Content-Type": "application/json"
    }
    payload = {
        "model": config['model'],
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 1000  # Adjust for response length
    }
    
    try:
        response = requests.post(url, headers=headers, json=payload)
        response.raise_for_status()
        return response.json()['choices'][0]['message']['content']
    except requests.RequestException as e:
        print(f"Grok API error: {e}")
        return None

if __name__ == "__main__":
    rosters = {"MyTeam": ["Player1", "Player2"]}
    suggestion = query_grok("waiver", rosters)
    print(f"Waiver Suggestions: {suggestion}")