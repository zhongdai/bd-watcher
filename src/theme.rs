use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub status_open: Color,
    pub status_in_progress: Color,
    pub status_blocked: Color,
    pub status_closed: Color,
    pub status_deferred: Color,
    pub progress_filled: Color,
    pub progress_empty: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub error: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ThemeName {
    Default,
    Light,
    SolarizedDark,
    SolarizedLight,
    Gruvbox,
    Dracula,
    HighContrast,
}

impl ThemeName {
    pub fn theme(self) -> Theme {
        match self {
            ThemeName::Default => default(),
            ThemeName::Light => light(),
            ThemeName::SolarizedDark => solarized_dark(),
            ThemeName::SolarizedLight => solarized_light(),
            ThemeName::Gruvbox => gruvbox(),
            ThemeName::Dracula => dracula(),
            ThemeName::HighContrast => high_contrast(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default" => Some(Self::Default),
            "light" => Some(Self::Light),
            "solarized-dark" | "solarized_dark" => Some(Self::SolarizedDark),
            "solarized-light" | "solarized_light" => Some(Self::SolarizedLight),
            "gruvbox" => Some(Self::Gruvbox),
            "dracula" => Some(Self::Dracula),
            "high-contrast" | "high_contrast" => Some(Self::HighContrast),
            _ => None,
        }
    }
}

fn default() -> Theme {
    Theme {
        bg: Color::Reset,
        fg: Color::Gray,
        muted: Color::DarkGray,
        accent: Color::Cyan,
        status_open: Color::Gray,
        status_in_progress: Color::Yellow,
        status_blocked: Color::Red,
        status_closed: Color::Green,
        status_deferred: Color::Blue,
        progress_filled: Color::Green,
        progress_empty: Color::DarkGray,
        selection_bg: Color::DarkGray,
        selection_fg: Color::White,
        error: Color::Red,
    }
}

fn light() -> Theme {
    Theme {
        bg: Color::White,
        fg: Color::Black,
        muted: Color::Gray,
        accent: Color::Blue,
        status_open: Color::DarkGray,
        status_in_progress: Color::Rgb(176, 112, 0),
        status_blocked: Color::Rgb(176, 0, 0),
        status_closed: Color::Rgb(0, 120, 0),
        status_deferred: Color::Blue,
        progress_filled: Color::Rgb(0, 120, 0),
        progress_empty: Color::Gray,
        selection_bg: Color::Rgb(220, 220, 255),
        selection_fg: Color::Black,
        error: Color::Red,
    }
}

fn solarized_dark() -> Theme {
    Theme {
        bg: Color::Rgb(0, 43, 54),
        fg: Color::Rgb(131, 148, 150),
        muted: Color::Rgb(88, 110, 117),
        accent: Color::Rgb(38, 139, 210),
        status_open: Color::Rgb(147, 161, 161),
        status_in_progress: Color::Rgb(181, 137, 0),
        status_blocked: Color::Rgb(220, 50, 47),
        status_closed: Color::Rgb(133, 153, 0),
        status_deferred: Color::Rgb(38, 139, 210),
        progress_filled: Color::Rgb(133, 153, 0),
        progress_empty: Color::Rgb(88, 110, 117),
        selection_bg: Color::Rgb(7, 54, 66),
        selection_fg: Color::Rgb(253, 246, 227),
        error: Color::Rgb(220, 50, 47),
    }
}

fn solarized_light() -> Theme {
    Theme {
        bg: Color::Rgb(253, 246, 227),
        fg: Color::Rgb(101, 123, 131),
        muted: Color::Rgb(147, 161, 161),
        accent: Color::Rgb(38, 139, 210),
        status_open: Color::Rgb(88, 110, 117),
        status_in_progress: Color::Rgb(181, 137, 0),
        status_blocked: Color::Rgb(220, 50, 47),
        status_closed: Color::Rgb(133, 153, 0),
        status_deferred: Color::Rgb(38, 139, 210),
        progress_filled: Color::Rgb(133, 153, 0),
        progress_empty: Color::Rgb(238, 232, 213),
        selection_bg: Color::Rgb(238, 232, 213),
        selection_fg: Color::Rgb(0, 43, 54),
        error: Color::Rgb(220, 50, 47),
    }
}

fn gruvbox() -> Theme {
    Theme {
        bg: Color::Rgb(40, 40, 40),
        fg: Color::Rgb(235, 219, 178),
        muted: Color::Rgb(146, 131, 116),
        accent: Color::Rgb(131, 165, 152),
        status_open: Color::Rgb(189, 174, 147),
        status_in_progress: Color::Rgb(250, 189, 47),
        status_blocked: Color::Rgb(251, 73, 52),
        status_closed: Color::Rgb(184, 187, 38),
        status_deferred: Color::Rgb(131, 165, 152),
        progress_filled: Color::Rgb(184, 187, 38),
        progress_empty: Color::Rgb(60, 56, 54),
        selection_bg: Color::Rgb(60, 56, 54),
        selection_fg: Color::Rgb(251, 241, 199),
        error: Color::Rgb(251, 73, 52),
    }
}

fn dracula() -> Theme {
    Theme {
        bg: Color::Rgb(40, 42, 54),
        fg: Color::Rgb(248, 248, 242),
        muted: Color::Rgb(98, 114, 164),
        accent: Color::Rgb(139, 233, 253),
        status_open: Color::Rgb(189, 189, 190),
        status_in_progress: Color::Rgb(241, 250, 140),
        status_blocked: Color::Rgb(255, 85, 85),
        status_closed: Color::Rgb(80, 250, 123),
        status_deferred: Color::Rgb(189, 147, 249),
        progress_filled: Color::Rgb(80, 250, 123),
        progress_empty: Color::Rgb(68, 71, 90),
        selection_bg: Color::Rgb(68, 71, 90),
        selection_fg: Color::Rgb(248, 248, 242),
        error: Color::Rgb(255, 85, 85),
    }
}

fn high_contrast() -> Theme {
    Theme {
        bg: Color::Black,
        fg: Color::White,
        muted: Color::Gray,
        accent: Color::Rgb(0, 255, 255),
        status_open: Color::White,
        status_in_progress: Color::Rgb(255, 221, 0),
        status_blocked: Color::Rgb(255, 64, 64),
        status_closed: Color::Rgb(64, 255, 64),
        status_deferred: Color::Rgb(128, 192, 255),
        progress_filled: Color::Rgb(64, 255, 64),
        progress_empty: Color::Rgb(64, 64, 64),
        selection_bg: Color::Rgb(0, 128, 255),
        selection_fg: Color::White,
        error: Color::Rgb(255, 64, 64),
    }
}

pub fn resolve(cli: Option<ThemeName>, env: Option<&str>) -> Theme {
    if let Some(name) = cli {
        return name.theme();
    }
    if let Some(v) = env {
        if let Some(name) = ThemeName::parse(v) {
            return name.theme();
        }
    }
    ThemeName::Default.theme()
}
