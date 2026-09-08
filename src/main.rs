//! Point d'entrée : initialise le terminal, installe les filets de sécurité
//! (hook de panique, désactivation propre de la capture souris) et lance la
//! boucle applicative. Toute la logique de jeu et d'affichage vit dans
//! `app` et `ui` ; ce fichier ne fait que la plomberie de démarrage/arrêt.

mod ai;
mod app;
mod event_thread;
mod game;
mod ui;

use std::io::{self, stdout};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;

/// Installe un second hook de panique, au-dessus de celui que
/// `ratatui::init()` a déjà posé (qui restaure raw mode + écran alternatif
/// avant d'afficher le message). `ratatui::init()` ne sait rien de la
/// capture souris qu'on active nous-mêmes juste après : sans ce hook
/// supplémentaire, une panique laisserait le terminal du correcteur en mode
/// "capture souris" (clics interprétés comme des séquences d'échappement),
/// ce qui est aussi inutilisable qu'un mode brut non restauré.
///
/// `std::panic::take_hook` récupère le hook déjà en place (celui de
/// `ratatui`) ; on le rappelle après avoir désactivé la capture souris, pour
/// ne rien perdre de la restauration qu'il effectue déjà.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        previous(info);
    }));
}

fn main() -> io::Result<()> {
    // `ratatui::init()` active le mode brut, l'écran alternatif, et installe
    // déjà un hook de panique qui restaure tout cela. On empile notre propre
    // hook par-dessus (voir `install_panic_hook`) avant de toucher au reste.
    let mut terminal = ratatui::init();
    install_panic_hook();

    // La capture souris n'est pas activée par défaut par `ratatui::init()` :
    // on l'active explicitement, et on prend soin de toujours la désactiver
    // avant de rendre la main, panique ou pas (voir `install_panic_hook` et
    // le bloc de restauration ci-dessous).
    let _ = execute!(stdout(), EnableMouseCapture);

    let events = event_thread::EventThread::new(250);
    let result = app::run(&mut terminal, &events);

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();

    result
}
