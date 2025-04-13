use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use pyo3::exceptions::PyValueError;

use othello_ai::{AI, AlphaBetaAI, RandomAI};
use othello_game::{Board, Colour, DefaultGame, Move, Pos, Score};

// Helper to convert row, col to 0-63 representation or 0 for pass
fn move_to_u8(mov: Option<Move>) -> u8 {
    match mov {
        Some(m) => (m.row * 8 + m.col + 1) as u8, // 1-64
        None => 0, // Pass
    }
}

// Helper to convert 0-64 representation back to Move or None for pass
// Requires the current player's colour
fn u8_to_move(move_repr: u8, player: Colour) -> PyResult<Option<Move>> {
    match move_repr {
        0 => Ok(None), // Represents pass intent
        1..=64 => {
            let index = move_repr - 1;
            let row = (index / 8) as Pos;
            let col = (index % 8) as Pos;
             // Basic bounds check, detailed validation happens later
            if row >= 0 && row < 8 && col >= 0 && col < 8 {
                 Ok(Some(Move { player, row, col }))
            } else {
                 Err(PyValueError::new_err(format!("Invalid move number: {}", move_repr)))
            }
        }
         _ => Err(PyValueError::new_err(format!("Move must be between 0 and 64, got {}", move_repr))),
    }
}

#[pyclass(name = "OthelloGame")]
struct PyOthelloGame {
    game: DefaultGame,
    // Store moves as the u8 representation (0 for pass, 1-64 for place)
    // We could store the actual Move structs but u8 is simpler for the Python API
    move_history: Vec<u8>,
}

#[pymethods]
impl PyOthelloGame {
    #[new]
    fn new() -> Self {
        PyOthelloGame {
            game: DefaultGame::new(),
            move_history: Vec::new(),
        }
    }

    /// List all moves made so far. 0 represents a pass, 1-64 represent placing a stone.
    #[getter]
    fn list_moves(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        // PyList::new is deprecated, use Bound API
        Ok(PyList::new_bound(py, &self.move_history).into())
    }

    /// Add a stone placement (1-64) or a pass (0).
    /// Returns true if the move was valid and applied, false otherwise.
    fn add_stone(&mut self, move_repr: u8) -> PyResult<bool> {
        let current_player = self.game.next_turn;
        let valid_moves: Vec<Move> = self.game.valid_moves(current_player).into_iter().collect();

        match u8_to_move(move_repr, current_player)? {
            Some(potential_move) => {
                // Check if the proposed move is in the list of valid moves
                if valid_moves.contains(&potential_move) {
                    self.game = self.game.apply(potential_move);
                    self.move_history.push(move_repr);
                    Ok(true)
                } else {
                    // Illegal placement
                    Ok(false)
                }
            }
            None => { // User wants to pass (move_repr == 0)
                // Pass is only valid if there are no other moves
                if valid_moves.is_empty() {
                    // Apply the "pass" by switching the turn without changing the board
                    self.game.next_turn = self.game.next_turn.opponent();
                    // Check if the *new* player also has no moves (game over condition)
                     if self.game.valid_moves(self.game.next_turn).into_iter().next().is_none() {
                        // Game is over, turn doesn't advance further in a real pass scenario
                        // but we keep the opponent's colour as next_turn to signify game end
                    }
                    self.move_history.push(0);
                    Ok(true)
                } else {
                    // Cannot pass if other moves are available
                    Ok(false)
                }
            }
        }
    }


    /// Have the AI determine the next move, apply it, and return the move representation (0-64).
    /// Strength corresponds to the search depth for AlphaBetaAI (e.g., 1-5).
    /// If strength is 0 or less, RandomAI is used. Returns None if no move is possible for AI (incl. game over).
    fn ai_move(&mut self, strength: Option<i32>) -> PyResult<Option<u8>> {
        let current_player = self.game.next_turn;
        let valid_moves: Vec<Move> = self.game.valid_moves(current_player).into_iter().collect();

        if valid_moves.is_empty() {
            // Current player must pass
            self.game.next_turn = current_player.opponent();
            // Check if opponent also has no moves -> game over
            if self.game.valid_moves(self.game.next_turn).into_iter().next().is_none() {
                // Game is over, no move made by AI
                self.move_history.push(0); // Record the pass
                return Ok(None); // No AI move applied
            } else {
                // Opponent *can* move, so the pass was successful
                self.move_history.push(0); // Record the pass
                return Ok(Some(0)); // Return 0 to signify the pass
            }
        }

        // If we reach here, there are valid moves for the current player
        // Determine the move without using dyn AI
        let chosen_move_struct = if let Some(s) = strength {
            if s > 0 {
                let ai = AlphaBetaAI { max_depth: s as usize };
                ai.choose_move(&self.game)
            } else {
                let ai = RandomAI {};
                ai.choose_move(&self.game)
            }
        } else {
            // Default to RandomAI if strength is not provided
            let ai = RandomAI {};
            ai.choose_move(&self.game)
        };

        if let Some(mov) = chosen_move_struct {
            // Ensure the AI's chosen move is actually valid (should always be if AI is correct)
            // Note: valid_moves check might be redundant if AI guarantees valid moves,
            // but keep for safety.
            if valid_moves.contains(&mov) {
                self.game = self.game.apply(mov);
                let move_repr = move_to_u8(Some(mov));
                self.move_history.push(move_repr);
                Ok(Some(move_repr))
            } else {
                // This case indicates an internal error or AI bug
                 Err(PyValueError::new_err(format!("AI chose an invalid move: {:?}", mov)))
            }
        } else {
             // This case implies the AI failed to choose a move despite valid_moves not being empty
             // Could happen if the AI logic itself has a bug or edge case
             Err(PyValueError::new_err("AI failed to choose a move despite available options"))
        }
    }

    /// Get the current board state as a list of 64 integers.
    /// 0: Empty, 1: Black, 2: White
    #[getter]
    fn board(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let mut board_repr: Vec<u8> = Vec::with_capacity(64);
        for r in 0..8 {
            for c in 0..8 {
                let piece = self.game.board.get(r, c);
                board_repr.push(match piece {
                    None => 0,
                    Some(Colour::Black) => 1,
                    Some(Colour::White) => 2,
                });
            }
        }
        // PyList::new is deprecated, use Bound API
        Ok(PyList::new_bound(py, &board_repr).into())
    }

    /// Get the color of the next player (1 for Black, 2 for White).
    #[getter]
    fn next_player(&self) -> PyResult<u8> {
        Ok(match self.game.next_turn {
            Colour::Black => 1,
            Colour::White => 2,
        })
    }

    /// Get the current scores as a tuple (black_score, white_score).
    #[getter]
    fn scores(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        let scores: (Score, Score) = self.game.scores();
        // Use Bound API for PyTuple::new
        Ok(PyTuple::new_bound(py, &[scores.0.into_py(py), scores.1.into_py(py)]).into())
    }


    /// Check if the game is over (neither player has any valid moves).
    #[getter]
    fn is_game_over(&self) -> PyResult<bool> {
        let current_player_has_moves = self.game.valid_moves(self.game.next_turn).into_iter().next().is_some();
        if current_player_has_moves {
            Ok(false) // Current player can move, game not over
        } else {
            // Current player must pass, check opponent
            let opponent_has_moves = self.game.valid_moves(self.game.next_turn.opponent()).into_iter().next().is_some();
             Ok(!opponent_has_moves) // Game is over if opponent also has no moves
        }
    }

    // Implement __str__ manually since Game doesn't implement Display
    fn __str__(&self) -> String {
        let mut s = String::with_capacity(8 * 9); // 8 rows * (8 chars + newline)
         for r in 0..8 {
             for c in 0..8 {
                 let piece = self.game.board.get(r, c);
                 s.push(match piece {
                     Some(Colour::Black) => 'B', // Using B/W for clarity
                     Some(Colour::White) => 'W',
                     _ => '.',
                 });
             }
             s.push('\n');
         }
        // Add score and next player info
        let scores = self.game.scores();
         s.push_str(&format!("Score: B {} - W {}\n", scores.0, scores.1));
         s.push_str(&format!("Next Turn: {}\n", if self.game.next_turn == Colour::Black {"Black"} else {"White"}));
        s
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn othello_rust(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyOthelloGame>()?;
    Ok(())
}
