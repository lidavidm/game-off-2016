extern crate termion;
extern crate thread_scoped;
extern crate time;
extern crate voodoo;

mod ai;
mod info_view;
mod map_view;
mod level;
mod player;
mod player_turn;
mod program;

use std::io::{Write};
use std::sync::mpsc::channel;

use termion::event::{Key, Event, MouseEvent};
use termion::input::{TermRead};

use voodoo::color::ColorValue;
use voodoo::window::{Point};

use info_view::InfoView;
use map_view::MapView;
use level::Level;
use player::Player;
use program::{Ability, Program, StatusEffect, Team};

const LEVEL_DESCR: [&'static str; 20] = [
    "                                                          ",
    "                                                          ",
    "          .........................                       ",
    "          .........................                       ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "   o................             .....                    ",
    "   o................             .....                    ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "          ..                     ..                       ",
    "          .........................                       ",
    "          .........................                       ",
    "                                                          ",
    "                                                          ",
    "                                                          ",
];

#[derive(Clone,Copy,Debug)]
pub enum UiState {
    Unselected,
    Selected,
    SelectTarget(Ability),
    Animating,
}

pub enum UiEvent {
    Quit,
    Tick,
    ClickMap(Point),
    ClickInfo(Point),
    EndTurn,
}

enum GameState {
    // Setup,
    PlayerTurn,
    AITurn,
    AITurnTransition,
    PlayerTurnTransition,
    // AIExecute,
    Quit,
}

pub struct ModelView {
    info: InfoView,
    map: MapView,
    player: Player,
}

struct State(GameState, UiState);

impl State {
    fn translate_event(&self, event: termion::event::Event, _level: &mut Level, mv: &mut ModelView) -> Option<UiEvent> {
        use GameState::*;
        match (self, event) {
            (_, Event::Key(Key::Char('q'))) => Some(UiEvent::Quit),
            (&State(PlayerTurn, _), Event::Mouse(MouseEvent::Press(_, x, y))) => {
                if let Some(p) = mv.map.from_global_frame(Point::new(x, y)) {
                    Some(UiEvent::ClickMap(p))
                }
                else if let Some(p) = mv.info.from_global_frame(Point::new(x, y)) {
                    if p.y == 23 {
                        Some(UiEvent::EndTurn)
                    }
                    else {
                        Some(UiEvent::ClickInfo(p))
                    }
                }
                else {
                    None
                }
            }
            _ => None,
        }
    }

    fn next(self, event: termion::event::Event, level: &mut Level, mv: &mut ModelView) -> State {
        use GameState::*;
        if let Some(event) = self.translate_event(event, level, mv) {
            match (self, event) {
                (State(PlayerTurn, _), UiEvent::EndTurn) => State(AITurnTransition, UiState::Unselected),
                (State(PlayerTurn, ui), event) => Self::next_player_turn(ui, event, level, mv),
                (State(AITurnTransition, ui), _) => State(AITurnTransition, ui),
                (State(AITurn, ui), _) => State(AITurn, ui),
                _ => unimplemented!(),
            }
        }
        else {
            self
        }
    }

    fn tick(self, level: &mut Level, mv: &mut ModelView) -> State {
        use GameState::*;
        match self {
            State(PlayerTurn, ui) => Self::next_player_turn(ui, UiEvent::Tick, level, mv),
            State(AITurnTransition, _) => {
                begin_turn(Team::Enemy, level, mv);
                State(AITurn, UiState::Unselected)
            }
            State(PlayerTurnTransition, _) => {
                begin_turn(Team::Player, level, mv);
                State(PlayerTurn, UiState::Unselected)
            }
            State(AITurn, UiState::Animating) => {
                let modified = update_programs(level, &mut mv.map);

                if !modified {
                    State(AITurn, UiState::Unselected)
                }
                else {
                    State(AITurn, UiState::Animating)
                }
            }
            State(AITurn, _) => {
                let ai_state = ai::ai_tick(level, &mut mv.map);
                mv.map.set_help(format!("AI STATUS: {:?}", ai_state));
                match ai_state {
                    ai::AIState::Done => State(PlayerTurnTransition, UiState::Unselected),
                    ai::AIState::Plotting => State(AITurn, UiState::Unselected),
                    ai::AIState::WaitingAnimation => State(AITurn, UiState::Animating),
                }
            },
            _ => unimplemented!(),
        }
    }

    fn next_player_turn(ui_state: UiState, event: UiEvent, level: &mut Level, mv: &mut ModelView) -> State {
        use GameState::*;
        match event {
            UiEvent::Quit => State(Quit, ui_state),
            UiEvent::ClickMap(_) | UiEvent::ClickInfo(_) | UiEvent::Tick => {
                State(PlayerTurn, player_turn::next(ui_state, event, level, mv))
            }
            UiEvent::EndTurn => unreachable!(),
        }
    }
}

fn begin_turn(team: Team, level: &mut Level, mv: &mut ModelView) {
    mv.info.set_team(team);
    mv.info.clear();
    mv.map.clear_range();
    mv.map.clear_highlight();
    mv.map.update_highlight(level);
    level.begin_turn();
}

pub fn update_programs(level: &mut Level, map: &mut MapView) -> bool {
    let mut modified = false;
    let mut killed = vec![];
    for program in level.programs.iter_mut() {
        let mut p = program.borrow_mut();
        let position = p.position;
        let mut damaged = false;
        for effect in p.status_effects.iter_mut() {
            match *effect {
                StatusEffect::Damage(damage) => {
                    modified = true;
                    damaged = true;
                    *effect = StatusEffect::Damage(damage - 1);
                }
            }
        }
        p.status_effects.retain(|effect| {
            match *effect {
                StatusEffect::Damage(0) => false,
                StatusEffect::Damage(_) => true,
            }
        });

        if damaged {
            let lived = p.damage();
            if !lived {
                killed.push(position);
                map.clear_highlight();
            }
        }
    }

    for position in killed {
        level.remove_program_at(position);
    }

    modified
}

const MS: u64 = 1_000_000;
const TICK_TIME: u64 = 250;

fn main() {
    use std::sync::mpsc::TryRecvError::*;
    use std::thread;
    use std::time::Duration;

    use voodoo::terminal::{Mode, Terminal};

    let mut level = Level::new(&LEVEL_DESCR);
    level.add_program(Program::new(Team::Player, Point::new(11, 9), "Hack"));
    level.add_program(Program::new(Team::Player, Point::new(5, 10), "Hack"));
    level.add_program(Program::new(Team::Enemy, Point::new(7, 10), "Hack"));
    level.add_program(Program::new(Team::Enemy, Point::new(9, 10), "Hack"));

    let mut terminal = Terminal::new();
    terminal.cursor(Mode::Disabled);
    terminal.clear_color(ColorValue::Black);
    let Terminal { ref mut stdin, ref mut stdout } = terminal;

    stdout.flush().unwrap();

    let mut info = voodoo::window::Window::new(Point::new(0, 0), 20, 24);
    let mut map = voodoo::window::Window::new(Point::new(20, 0), 60, 24);
    info.border();
    map.border();

    let mut info_view = InfoView::new(info);
    let mut map_view = MapView::new(map);
    info_view.refresh(stdout);
    map_view.display(&level);
    map_view.refresh(stdout);

    let player = Player::new("David");

    let mut mv = ModelView {
        info: info_view,
        map: map_view,
        player: player,
    };
    let ui_state = UiState::Unselected;

    let mut state = State(GameState::PlayerTurnTransition, ui_state);

    let (tx, rx) = channel();
    let guard = unsafe {
        thread_scoped::scoped(move || {
            for c in stdin.events() {
                let evt = c.unwrap();
                if let Event::Key(Key::Char('q')) = evt {
                    break;
                }
                tx.send(evt).unwrap();
            }
        })
    };

    let mut t = time::precise_time_ns();
    let mut dt = 0;

    'main: loop {
        loop {
            // Handle all pending events
            let msg = rx.try_recv();
            match msg {
                Ok(evt) => {
                    state = state.next(evt, &mut level, &mut mv);
                    if let State(GameState::Quit, _) = state {
                        break 'main;
                    }
                },
                Err(Disconnected) => break 'main,
                Err(Empty) => break,
            }
        }

        let now = time::precise_time_ns();
        dt += now - t;

        // TODO: use constant
        while dt >= TICK_TIME * MS {
            state = state.tick(&mut level, &mut mv);
            dt -= TICK_TIME * MS;
        }

        mv.info.refresh(stdout);
        mv.map.display(&level);
        mv.map.refresh(stdout);
        t = now;

        thread::sleep(Duration::from_millis((TICK_TIME - dt / MS) / 2));
    }
    guard.join();
}
