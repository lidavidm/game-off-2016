use termion;
use termion::event::{Key, Event, MouseEvent};

use voodoo;
use voodoo::window::{Point};

use ai;
use data;
use info_view::{self, InfoView};
use level_transition;
use map_view::MapView;
use mission_select;
use level::Level;
use player::Player;
use player_turn;
use program::{Ability, Program, StatusEffect, Team};


#[derive(Clone,Copy,Debug)]
pub enum UiState {
    Unselected,
    Selected,
    SelectTarget(Ability),
    Animating,
}

#[derive(Clone,Copy,Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone,Copy,Debug)]
pub enum UiEvent {
    Quit,
    Tick,
    ClickMap(Point),
    ClickInfo(Point),
    Move(Direction),
    EndTurn,
}

#[derive(Debug)]
pub enum GameState {
    Setup(UiState),
    PlayerTurn(UiState),
    AITurn(UiState),
    SetupTransition,
    AITurnTransition,
    PlayerTurnTransition,
    Quit,
    MissionSelect(mission_select::State),
    LevelTransition(level_transition::State),
}

pub struct ModelView {
    pub level_index: usize,
    pub info: InfoView,
    pub map: MapView,
    pub player: Player,
    pub program_list: info_view::ChoiceList<Program>,
    pub level: Level,
}

impl GameState {
    pub fn translate_event(&self, event: Event, mv: &mut ModelView) -> Option<UiEvent> {
        match (self, event) {
            (_, Event::Key(Key::Char('q'))) => Some(UiEvent::Quit),
            (_, Event::Key(Key::Char('w'))) => Some(UiEvent::Move(Direction::Up)),
            (_, Event::Key(Key::Char('s'))) => Some(UiEvent::Move(Direction::Down)),
            (_, Event::Key(Key::Char('a'))) => Some(UiEvent::Move(Direction::Left)),
            (_, Event::Key(Key::Char('d'))) => Some(UiEvent::Move(Direction::Right)),
            (&GameState::PlayerTurn(_), Event::Mouse(MouseEvent::Press(_, x, y))) |
            (&GameState::Setup(_), Event::Mouse(MouseEvent::Press(_, x, y))) => {
                if let Some(p) = mv.map.from_global_frame(Point::new(x - 1, y - 1)) {
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

    pub fn next(self, event: termion::event::Event, mv: &mut ModelView) -> GameState {
        match (self, event) {
            (GameState::MissionSelect(ms), Event::Key(_)) => Self::next_mission_turn(ms, mission_select::UiEvent::KeyPressed, mv),
            (GameState::LevelTransition(lt), Event::Key(_)) => Self::next_transition_turn(lt, level_transition::UiEvent::KeyPressed, mv),
            (state, _) => {
                if let Some(event) = state.translate_event(event, mv) {
                    match state {
                        GameState::Setup(ui) => Self::next_setup_turn(ui, event, mv),
                        GameState::PlayerTurn(ui) => match event {
                            UiEvent::EndTurn => {
                                match mv.level.check_victory() {
                                    Some(team) => GameState::LevelTransition(level_transition::State::new(mv.level_index, team)),
                                    None => GameState::AITurnTransition
                                }
                            },
                            _ => Self::next_player_turn(ui, event, mv)
                        },
                        GameState::MissionSelect(_) | GameState::LevelTransition(_) => state,
                        GameState::SetupTransition |
                        GameState::AITurnTransition | GameState::PlayerTurnTransition |
                        GameState::AITurn(_) | GameState::Quit => state,
                    }
                }
                else {
                    state
                }
            }
        }
    }

    pub fn tick(self, mv: &mut ModelView) -> GameState {
        match self {
            GameState::Setup(ui) => Self::next_setup_turn(ui, UiEvent::Tick, mv),
            GameState::PlayerTurn(ui) => Self::next_player_turn(ui, UiEvent::Tick, mv),
            GameState::MissionSelect(ms) => Self::next_mission_turn(ms, mission_select::UiEvent::Tick, mv),
            GameState::LevelTransition(lt) => Self::next_transition_turn(lt, level_transition::UiEvent::Tick, mv),
            GameState::AITurnTransition => {
                begin_turn(Team::Enemy, mv);
                GameState::AITurn(UiState::Unselected)
            }
            GameState::PlayerTurnTransition => {
                match mv.level.check_victory() {
                    Some(team) => GameState::LevelTransition(level_transition::State::new(mv.level_index, team)),
                    None => {
                        begin_turn(Team::Player, mv);
                        GameState::PlayerTurn(UiState::Unselected)
                    }
                }
            }
            GameState::AITurn(UiState::Animating) => {
                let modified = update_programs(&mut mv.level, &mut mv.map);

                if !modified {
                    GameState::AITurn(UiState::Unselected)
                }
                else {
                    GameState::AITurn(UiState::Animating)
                }
            }
            GameState::AITurn(_) => {
                let ai_state = ai::ai_tick(&mut mv.level, &mut mv.map);
                mv.map.set_help(format!("AI STATUS: {:?}", ai_state));
                match ai_state {
                    ai::AIState::Done => GameState::PlayerTurnTransition,
                    ai::AIState::Plotting => GameState::AITurn(UiState::Unselected),
                    ai::AIState::WaitingAnimation => GameState::AITurn(UiState::Animating),
                }
            }
            GameState::SetupTransition => {
                mv.map.reset();
                mv.info.clear();
                mv.info.primary_action = ">Launch Intrusion<".to_owned();
                mv.info.display_end_turn();
                mv.map.display(&mv.level);
                mv.program_list.choices().clear();
                mv.program_list.choices().extend(mv.player.programs.iter().map(|x| {
                    (x.name.to_owned(), x.clone())
                }));
                begin_turn(Team::Player, mv);
                GameState::Setup(UiState::Unselected)
            }
            GameState::Quit => self,
        }
    }

    pub fn display(&mut self, compositor: &mut voodoo::compositor::Compositor, mv: &mut ModelView) {
        use self::GameState::*;

        match self {
            &mut MissionSelect(ref mut state) => {
                mission_select::display(state, compositor, mv);
            }
            &mut LevelTransition(ref mut state) => {
                level_transition::display(state, compositor, mv);
            }
            _ => {
                mv.info.refresh(compositor);
                mv.map.display(&mv.level);
                mv.map.refresh(compositor);
            }
        }
    }

    pub fn next_player_turn(ui_state: UiState, event: UiEvent, mv: &mut ModelView) -> GameState {
        match event {
            UiEvent::ClickMap(_) | UiEvent::ClickInfo(_) | UiEvent::Tick | UiEvent::Move(_) => {
                GameState::PlayerTurn(player_turn::next(ui_state, event, mv))
            }
            UiEvent::EndTurn | UiEvent::Quit => unreachable!(),
        }
    }

    pub fn next_mission_turn(mut mission_state: mission_select::State, event: mission_select::UiEvent, mv: &mut ModelView) -> GameState {
        match mission_select::next(&mut mission_state, event, mv) {
            mission_select::Transition::Ui(_) => GameState::MissionSelect(mission_state),
            mission_select::Transition::Level(index) => {
                if let Some(level) = data::load_level(index) {
                    mv.level_index = index;
                    mv.level = level;
                    GameState::SetupTransition
                }
                else {
                    GameState::Quit
                }
            }
        }
    }

    pub fn next_transition_turn(mut state: level_transition::State, event: level_transition::UiEvent, mv: &mut ModelView) -> GameState {
        match level_transition::next(&mut state, event, mv) {
            Some(index) => {
                if let Some(level) = data::load_level(index) {
                    mv.level_index = index;
                    mv.level = level;
                    GameState::SetupTransition
                }
                else {
                    GameState::Quit
                }
            },
            None => {
                GameState::LevelTransition(state)
            }
        }
    }

    pub fn next_setup_turn(ui_state: UiState, event: UiEvent, mv: &mut ModelView) -> GameState {
        match event {
            UiEvent::ClickMap(_) | UiEvent::ClickInfo(_) | UiEvent::Tick | UiEvent::Move(_) => {
                GameState::Setup(player_turn::next_setup(ui_state, event, mv))
            }
            UiEvent::EndTurn => {
                // TODO: reset
                mv.info.primary_action = ">    End Turn    <".to_owned();
                mv.info.display_end_turn();
                GameState::PlayerTurnTransition
            }
            UiEvent::Quit => unreachable!(),
        }
    }
}

pub fn begin_turn(team: Team, mv: &mut ModelView) {
    mv.info.set_team(team);
    mv.info.clear();
    mv.map.clear_range();
    mv.map.clear_highlight();
    mv.map.update_highlight(&mut mv.level);
    mv.level.begin_turn();
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
