import requests
import yaml
import json
from espn_fetcher import get_league  # To get current_week dynamically

def load_grok_config():
    with open('config.yaml', 'r') as f:
        return yaml.safe_load(f)['grok']

def load_yaml_file(file_path):
    if not os.path.exists(file_path):
        return None
    with open(file_path, 'r') as f:
        return yaml.safe_load(f)

def query_grok(prompt, data=None, include_league_data=True):
    config = load_grok_config()
    full_prompt = prompt

    if include_league_data:
        # Load league data from YAML files
        activity_data = load_yaml_file('league_activity.yaml') or {}
        standings_data = load_yaml_file('league_standings.yaml') or {}
        current_week_data = load_yaml_file('league_current_week.yaml') or {}

        # Dynamically load previous week result
        league = get_league()
        previous_week = max(league.current_week - 1, 1)
        week_result_data = load_yaml_file(f'league_week{previous_week}_result.yaml') or {}

        # Append data to prompt
        full_prompt += "\n\nLeague Context:"
        full_prompt += f"\nRecent Activity (Trades/Free Agents): {json.dumps(activity_data)}"
        full_prompt += f"\nStandings (Points/Expected Finish): {json.dumps(standings_data)}"
        full_prompt += f"\nCurrent Week Matchups (Estimated Points): {json.dumps(current_week_data)}"
        full_prompt += f"\nPrevious Week Results (Player Points): {json.dumps(week_result_data)}"

    if data:
        full_prompt += f"\nAdditional Data: {json.dumps(data)}"

    url = "https://api.x.ai/v1/chat/completions"
    headers = {
        "Authorization": f"Bearer {config['api_key']}",
        "Content-Type": "application/json"
    }
    payload = {
        "model": config['model'],
        "messages": [{"role": "user", "content": full_prompt}]
    }
    response = requests.post(url, headers=headers, json=payload)
    if response.status_code == 200:
        return response.json()['choices'][0]['message']['content']
    else:
        raise Exception(f"API error: {response.text}")

if __name__ == "__main__":
    rosters = {"MyTeam": ["Player1", "Player2"]}
    suggestion = query_grok("Suggest waiver pickups.", rosters)
    print(suggestion)