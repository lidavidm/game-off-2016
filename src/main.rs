extern crate ncurses;
extern crate voodoo;

use ncurses::*;
use voodoo::terminal::{Mode, Terminal};
use voodoo::window::{DisplayChar, Window, WindowLike};

const LEVEL_DESCR: [&'static str; 22] = [
    "                                                          ",
    "                                                          ",
    "                                                          ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "   o................                                      ",
    "   o................                                      ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "          ..                                              ",
    "          .......................                         ",
    "          .......................                         ",
    "                                                          ",
    "                                                          ",
    "                                                          ",
    "                                                          ",
];

struct Program {
    y: i32,
    x: i32,
}

impl Program {
    fn new(y: i32, x: i32) -> Program {
        Program {
            y: y,
            x: x,
        }
    }

    fn display(&self, map: &mut Window) {
        map.put_at(self.y, self.x, ACS_BLOCK());
    }
}

struct Level {
    layout: Vec<String>,
    player_programs: Vec<Program>,
}

impl Level {
    fn new(description: &[&str; 22]) -> Level {
        let mut layout = Vec::new();
        for s in description.iter() {
            layout.push(s.to_string());
        }
        Level {
            layout: layout,
            player_programs: Vec::new(),
        }
    }

    fn display_for(&self, y: usize, x: usize) -> Option<DisplayChar> {
        Self::convert(self.layout[y].chars().nth(x).unwrap())
    }

    // TODO: need char -> Tile -> DisplayChar

    fn convert(c: char) -> Option<DisplayChar> {
        match c {
            '.' => Some(Into::<DisplayChar>::into(ACS_BULLET()).dim()), // '·'
            'o' => Some(Into::<DisplayChar>::into('O')),
            _ => None,
        }
    }

    fn display(&self, map: &mut Window) {
        for (y, line) in self.layout.iter().enumerate() {
            let y = y + 1;
            for (x, tile) in line.chars().enumerate() {
                let x = x + 1;
                match Self::convert(tile) {
                    Some(c) => map.put_at(y as i32, x as i32, c),
                    None => {},
                }
            }
        }

        for program in self.player_programs.iter() {
            program.display(map);
        }
    }
}

fn main() {
    let mut level = Level::new(&LEVEL_DESCR);
    level.player_programs.push(Program::new(4, 4));

    let term = Terminal::new();
    term.cbreak(Mode::Enabled).unwrap();
    term.echo(Mode::Disabled).unwrap();

    keypad(stdscr(), true);
    curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);

    mousemask((ALL_MOUSE_EVENTS | REPORT_MOUSE_POSITION) as u32, None);

    start_color();
    init_pair(1, COLOR_BLACK, COLOR_WHITE);

    wbkgd(stdscr(), 1);

    refresh();

    let mut info = Window::new(0, 0, 20, 24);
    let mut map = Window::new(20, 0, 60, 24);
    info.box_(0, 0);
    map.box_(0, 0);

    level.display(&mut map);

    info.refresh();
    map.refresh();

    print!("\x1B[?1003h\n"); // Makes the terminal report mouse movement events

    loop {
        match voodoo::poll_event() {
            Some(voodoo::Event::Mouse) => {
                let event = voodoo::get_mouse_state();
                map.put_at(1, 1, 'x');
                let x = event.x - 20;
                let y = event.y - 1;

                if y <= 0 || y >= 19 || x <= 0 || x >= 59 {
                }
                else if ((event.state as i32) & BUTTON1_CLICKED) != 0 {
                    map.put_at(y, x, 'a');
                }
                else if let Some(c) = level.display_for(event.y as usize - 1, event.x as usize - 21) {
                    map.put_at(event.y, event.x - 20, c.bold());
                }
                map.refresh();
            }

            Some(voodoo::Event::Char('\n')) => {
                break;
            }

            _ => {
                map.put_at(1, 1, 'o');
                map.refresh();
            }
        }
    }

    print!("\x1B[?1003l\n"); // Disable mouse movement events, as l = low
}
