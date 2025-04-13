from othello_rust import OthelloGame
import random


def play_game():
    game = OthelloGame()
    while True:
        if game.is_game_over:
            break
        if game.next_player == 2:
            game.ai_move(random.randint(1, 3))
        else:
            game.ai_move(0)
    return game.scores[0] > game.scores[1]


black_wins = 0
white_wins = 0
for i in range(1000):
    if play_game():
        black_wins += 1
    else:
        white_wins += 1
    print(black_wins, white_wins)