#[allow(dead_code)]
#[path = "../src/meta.rs"]
mod meta;

use meta::credit_featured_artists;

#[test]
fn files_fifty_cent_and_nate_dogg_under_fifty_cent() {
    let (title, artist) =
        credit_featured_artists("21 Questions".into(), Some("50 Cent/Nate Dogg".into()));

    assert_eq!(artist.as_deref(), Some("50 Cent"));
    assert_eq!(title, "21 Questions - featuring Nate Dogg");
}

#[test]
fn trims_slash_separated_names_and_joins_several_guests() {
    let (title, artist) = credit_featured_artists(
        "My Life".into(),
        Some(" 50 Cent / Eminem / Adam Levine ".into()),
    );

    assert_eq!(artist.as_deref(), Some("50 Cent"));
    assert_eq!(title, "My Life - featuring Eminem & Adam Levine");

    let (title, artist) = credit_featured_artists(
        "Spend Some Time".into(),
        Some("Eminem/Obie Trice/Stat Quo/50 Cent".into()),
    );
    assert_eq!(artist.as_deref(), Some("Eminem"));
    assert_eq!(
        title,
        "Spend Some Time - featuring Obie Trice, Stat Quo & 50 Cent"
    );
}

#[test]
fn leaves_guests_already_named_in_the_title() {
    let (title, artist) = credit_featured_artists(
        "Been a Long Time (feat. Nate Dogg)".into(),
        Some("Xzibit/Nate Dogg".into()),
    );

    assert_eq!(artist.as_deref(), Some("Xzibit"));
    assert_eq!(title, "Been a Long Time (feat. Nate Dogg)");
}

#[test]
fn leaves_solo_artists_unchanged() {
    let (title, artist) = credit_featured_artists("In Da Club".into(), Some("50 Cent".into()));

    assert_eq!(artist.as_deref(), Some("50 Cent"));
    assert_eq!(title, "In Da Club");
    assert_eq!(
        credit_featured_artists("Untitled".into(), None),
        ("Untitled".into(), None)
    );
}
