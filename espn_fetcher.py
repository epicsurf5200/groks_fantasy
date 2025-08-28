from espn_api.football import League
import yaml
import os

def load_config():
    with open('config.yaml', 'r') as f:
        return yaml.safe_load(f)['espn']

def get_league():
    config = load_config()
    return League(league_id=config['league_id'], year=config['season'],
                  swid=config['swid'], espn_s2=config['espn_s2'])

def serialize_activity(activity):
    return [{'date': act.date, 'actions': [(action[0].team_name if action[0] else None, action[1], action[2].name if action[2] else None) for action in act.actions]} for act in activity]

def serialize_standings(standings, power_rankings):
    return [{'team': team.team_name, 'wins': team.wins, 'losses': team.losses, 'points_for': team.points_for, 'points_against': team.points_against, 'expected_finish': rank} for rank, team in enumerate(power_rankings, 1)]

def serialize_current_week(matchups):
    return [{'home_team': m.home_team.team_name, 'away_team': m.away_team.team_name, 'home_projected': m.home_projected, 'away_projected': m.away_projected} for m in matchups]

def serialize_week_results(box_scores):
    results = {}
    for box in box_scores:
        results[box.home_team.team_name] = [{'player': p.name, 'position': p.position, 'points': p.points} for p in box.home_lineup]
        results[box.away_team.team_name] = [{'player': p.name, 'position': p.position, 'points': p.points} for p in box.away_lineup]
    return results

def update_data_files():
    league = get_league()
    current_week = league.current_week
    previous_week = max(current_week - 1, 1)

    # League Activity (trades and free agent acquisitions)
    activity = league.recent_activity(size=50)
    with open('league_activity.yaml', 'w') as f:
        yaml.safe_dump({'activity': serialize_activity(activity)}, f)

    # League Standings (standings, points, expected finish via power rankings)
    standings = league.standings()
    power_rankings = league.power_rankings(week=current_week)  # Using power rankings for expected finish approximation
    with open('league_standings.yaml', 'w') as f:
        yaml.safe_dump({'standings': serialize_standings(standings, power_rankings)}, f)

    # Current Week (games and estimated points)
    matchups = league.scoreboard(week=current_week)
    with open('league_current_week.yaml', 'w') as f:
        yaml.safe_dump({'current_week': current_week, 'matchups': serialize_current_week(matchups)}, f)

    # Previous Week Results (actual points per player)
    box_scores = league.box_scores(week=previous_week)
    with open(f'league_week{previous_week}_result.yaml', 'w') as f:
        yaml.safe_dump({'week': previous_week, 'results': serialize_week_results(box_scores)}, f)

def get_league_data():
    league = get_league()
    rosters = {team.team_name: [player.name for player in team.roster] for team in league.teams}
    return rosters

# Call update_data_files() in main.py before querying Grok
if __name__ == "__main__":
    update_data_files()