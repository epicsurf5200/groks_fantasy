import time
import os
import yaml
from espn_api.football import League
from grok_integration import query_grok

DATA_DIR = 'league_data'
DRAFT_PICKS_FILE = os.path.join(DATA_DIR, 'draft_picks.yaml')

def load_config():
    with open('config.yaml', 'r') as f:
        return yaml.safe_load(f)['espn']

def get_league():
    config = load_config()
    league = League(
        league_id=config['league_id'], 
        year=config['season'],
        swid=config['swid'], 
        espn_s2=config['espn_s2']
    )
    # Add user-agent header to avoid 403 errors
    league.headers = {'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36'}
    return league

def get_draft_picks(league):
    # Fetch the latest draft picks from ESPN API
    draft = league.draft  # Assuming the API has a draft attribute or method; adjust based on actual API
    picks = []
    for pick in draft:
        picks.append({
            'pick_number': pick.pick_no,
            'player': pick.playerName,
            'team': pick.team.team_name
        })
    return picks

def save_draft_picks(picks):
    os.makedirs(DATA_DIR, exist_ok=True)
    with open(DRAFT_PICKS_FILE, 'w') as f:
        yaml.safe_dump({'draft_picks': picks}, f)

def load_draft_picks():
    if os.path.exists(DRAFT_PICKS_FILE):
        with open(DRAFT_PICKS_FILE, 'r') as f:
            return yaml.safe_load(f).get('draft_picks', [])
    return []

def get_grok_suggestions(league, my_draft_order, current_pick, rosters):
    prompt = f"""
It's my turn to draft (pick {current_pick}, draft order {my_draft_order}).
Suggest 3 players to pick, based on:
- Positions needed on my team (analyze my current roster).
- Expert analysis for each player (rankings, projections).
- What other teams are drafting and the overall pool for each position.
- Avoid same bye weeks for same positions.
- Player schedule for even production each week.
- Look for handcuffs and sleepers.

My roster: {rosters.get('MyTeam', [])}
League draft picks so far: {load_draft_picks()}
"""
    suggestions = query_grok(prompt, include_league_data=True)
    return suggestions

def main():
    league = get_league()
    my_draft_order = int(input("Enter your draft order number: "))
    
    print("Monitoring draft...")
    last_picks = []
    
    while True:
        current_picks = get_draft_picks(league)
        if len(current_picks) > len(last_picks):
            # New pick made
            new_pick = current_picks[-1]
            print(f"New pick: {new_pick['pick_number']} - {new_pick['player']} by {new_pick['team']}")
            save_draft_picks(current_picks)
            last_picks = current_picks
        
        current_pick = len(current_picks) + 1
        if (current_pick - 1) % league.settings.team_count == my_draft_order - 1:
            # My turn
            print("It's your turn! Fetching suggestions...")
            rosters = get_league_data()
            suggestions = get_grok_suggestions(league, my_draft_order, current_pick, rosters)
            print(suggestions)
            
            # Wait for user to confirm pick
            while input("Press 'c' to continue after your pick: ").lower() != 'c':
                pass
            
            # Pull API to confirm user's pick
            updated_picks = get_draft_picks(league)
            if len(updated_picks) > len(last_picks):
                my_pick = updated_picks[-1]
                print(f"Your pick confirmed: {my_pick['player']}")
                save_draft_picks(updated_picks)
                last_picks = updated_picks
        
        time.sleep(1)  # Poll every second

if __name__ == "__main__":
    main()