use crate::error::MdError;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::{io, time::Duration, fs};
use crate::sysfs::MdSysfs;

pub fn run() -> Result<(), MdError> {
    // Setup terminal
    enable_raw_mode().map_err(MdError::Io)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(MdError::Io)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(MdError::Io)?;

    let res = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode().map_err(MdError::Io)?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ).map_err(MdError::Io)?;
    terminal.show_cursor().map_err(MdError::Io)?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(100)].as_ref())
                .split(f.size());

            let mut info = String::from("MD Arrays Status:\n\n");
            
            if let Ok(entries) = fs::read_dir("/sys/block") {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("md") {
                        let sys = MdSysfs::new(&name_str);
                        let state = sys.get_array_state()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|_| "unknown".to_string());
                        
                        info.push_str(&format!("🖴 {} - State: [{}]\n", name_str, state.to_uppercase()));
                    }
                }
            }

            let block = Paragraph::new(info)
                .block(Block::default()
                    .title(" rmdadm Live Monitor (Press 'q' to quit) ")
                    .borders(Borders::ALL));
            
            f.render_widget(block, chunks[0]);
        })?;

        // Poll for input every 500ms to allow refreshing array state
        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    return Ok(());
                }
            }
        }
    }
}
