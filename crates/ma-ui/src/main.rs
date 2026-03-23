//! milolew Audio — GUI entry point.

use vizia::prelude::*;

use ma_ui::app_data::AppData;
use ma_ui::views::root_view::RootView;

fn main() {
    env_logger::init();

    Application::new(|cx| {
        cx.add_stylesheet(include_str!("theme.css"))
            .expect("Failed to add theme stylesheet");

        AppData::new().build(cx);

        RootView::new(cx);
    })
    .title("milolew Audio")
    .inner_size((1280, 800))
    .run()
    .expect("Failed to run application");
}
