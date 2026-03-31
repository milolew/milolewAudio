# RustDAW — Specyfikacja GUI

## Spis treści

1. [Filozofia interfejsu](#1-filozofia-interfejsu)
2. [Wybór frameworka GUI](#2-wybór-frameworka-gui)
3. [Architektura okna głównego](#3-architektura-okna-głównego)
4. [Control Bar (pasek transportu)](#4-control-bar)
5. [Arrangement View (widok aranżacji)](#5-arrangement-view)
6. [Session View (widok sesji)](#6-session-view)
7. [Mixer (mikser)](#7-mixer)
8. [Piano Roll (edytor MIDI)](#8-piano-roll)
9. [Device Rack (łańcuch efektów/instrumentów)](#9-device-rack)
10. [Browser (przeglądarka plików)](#10-browser)
11. [Renderowanie waveform](#11-renderowanie-waveform)
12. [Widgety audio-specyficzne](#12-widgety-audio-specyficzne)
13. [Komunikacja GUI ↔ Audio Thread](#13-komunikacja-gui--audio-thread)
14. [System kolorów i theming](#14-system-kolorów-i-theming)
15. [Skróty klawiszowe i workflow](#15-skróty-klawiszowe-i-workflow)
16. [Roadmapa implementacji](#16-roadmapa-implementacji)

---

## 1. Filozofia interfejsu

### Cele projektowe

Interfejs wzoruje się na Ableton Live, ale nie kopiuje go 1:1. Priorytetem jest **funkcjonalność nad estetyką** w pierwszych fazach rozwoju. Trzy zasady przewodnie:

- **Ciemny, niskokontrastowy motyw** — oczy muzyka pracują godzinami, pastelowe kolory na ciemnym tle minimalizują zmęczenie wzroku. Jasne kolory zarezerwowane wyłącznie dla elementów aktywnych: playhead, zaznaczenia, aktywne nuty MIDI.
- **Gęstość informacji** — każdy piksel na ekranie powinien komunikować stan projektu. Waveformy, nuty, nazwy ścieżek, poziomy głośności — wszystko widoczne bez dodatkowych kliknięć.
- **Zero modali** — żadnych okien blokujących workflow. Wszystkie edytory (piano roll, device rack, browser) to panele dokowane w głównym oknie, przełączane jednym klawiszem.

### Wzorce z Ableton Live, które adaptujemy

Ableton opiera się na dualizmie **Session View** (siatka klipów do improwizacji i występów na żywo) i **Arrangement View** (liniowa oś czasu do aranżacji). Obie widoki współdzielą te same ścieżki — instrument dodany w Session View jest widoczny w Arrangement i odwrotnie. To przełączanie (`Tab`) jest kluczowe: Session służy do szkicowania pomysłów, Arrangement do finalizacji utworu.

Kluczowe elementy, które implementujemy:

- Przełączanie Session ↔ Arrangement jednym klawiszem
- Współdzielone ścieżki między widokami
- Device View (łańcuch efektów) zawsze na dole ekranu
- Browser po lewej stronie
- Control Bar (transport) zawsze na górze

### Czego NIE kopiujemy

- Grooves i warping audio (złożoność na poziomie osobnego projektu)
- Automation lanes w pierwszej wersji (dodamy w v2)
- Session View scenes (implementujemy uproszczoną wersję)

---

## 2. Wybór frameworka GUI

### Rekomendacja: vizia + custom wgpu widgets

Na podstawie analizy ekosystemu (Meadowlark post-mortem, porównania frameworków, wymagania DAW) rekomendowany stack to:

**vizia** jako główny framework z **custom widgetami renderowanymi przez wgpu** dla elementów wymagających wysokiej wydajności (waveformy, piano roll, mixer VU metry).

### Dlaczego vizia

| Cecha | Znaczenie dla DAW | vizia |
|-------|-------------------|-------|
| Deklaratywny model z lensami | Binding parametrów audio do UI | ✅ Natywne |
| CSS hot-reload | Szybka iteracja wyglądu | ✅ Pełne wsparcie |
| Rendering Skia | Jakość tekstu i grafiki wektorowej | ✅ Skia backend |
| Baseview backend | Osadzanie okien pluginów | ✅ Natywne |
| Knob/Slider widgety | Kontrola parametrów | ✅ Gotowe w ekosystemie nih-plug |
| Dojrzałość API | Stabilność projektu | ⚠️ Pre-1.0, ale aktywnie rozwijana |

### Alternatywa: iced + iced_audio

Jeśli vizia okaże się zbyt niestabilna, **iced** (v0.14+) jest silnym backup planem. Elm-architecture naturalnie mapuje się na undo/redo, a `iced_audio` dostarcza widgety audio (knobs, sliders, XY pads). Wersja 0.14 dodała reactive rendering (przerysowuje tylko zmienione widgety).

### Architektura hybrydowa

Elementy GUI dzielimy na dwie kategorie renderowania:

```
┌─────────────────────────────────────────────────┐
│  vizia (Skia)                                   │
│  ├── Control Bar, przyciski, labele, listy      │
│  ├── Browser (drzewo plików)                    │
│  ├── Device Rack (łańcuch pluginów)             │
│  └── Layouty paneli, splitters, tab switche     │
│                                                 │
│  Custom wgpu/Skia Canvas Widgets                │
│  ├── Arrangement Timeline (waveformy + clipy)   │
│  ├── Piano Roll (siatka nut)                    │
│  ├── Mixer VU Meters (60fps animacja)           │
│  └── Waveform Overview (miniatura projektu)     │
└─────────────────────────────────────────────────┘
```

Vizia pozwala na osadzanie custom widgetów z bezpośrednim dostępem do Skia canvas (`cx.draw(|canvas| { ... })`), więc nie potrzebujemy osobnego okna wgpu — rysujemy wewnątrz vizia.

---

## 3. Architektura okna głównego

### Layout (rozmieszczenie paneli)

```
┌──────────────────────────────────────────────────────────┐
│                    CONTROL BAR (40px)                     │
├────────┬─────────────────────────────────────────────────┤
│        │                                                 │
│        │         ARRANGEMENT VIEW                        │
│        │              lub                                │
│ BROWSER│         SESSION VIEW                            │
│ (250px)│         (przełączane Tab)                       │
│        │                                                 │
│        │                                                 │
│        ├─────────────────────────────────────────────────┤
│        │         DETAIL VIEW (dolny panel)               │
│        │    Piano Roll / Device Rack / Clip Editor       │
│        │         (przełączane, ~35% wysokości)           │
└────────┴─────────────────────────────────────────────────┘
```

### Splitter system

Wszystkie podziały paneli są przeciągalne (draggable splitters). Trzy główne splittery:

- **Pionowy** — granica Browser ↔ Main View (domyślnie 250px, min 150px, max 400px)
- **Poziomy** — granica Main View ↔ Detail View (domyślnie 65%/35%, min 100px każdy)
- **Browser toggle** — `Ctrl+Alt+B` chowa/pokazuje browser (animacja slide 200ms)

### Zarządzanie panelami

```rust
enum MainView {
    Arrangement,
    Session,
}

enum DetailView {
    None,           // schowany (double-click na splitterze)
    PianoRoll,      // edycja MIDI clipu
    AudioClipEditor,// edycja audio clipu (waveform + parametry)
    DeviceRack,     // łańcuch instrumentów/efektów
    Mixer,          // mikser (alternatywnie full-screen)
}

struct LayoutState {
    main_view: MainView,
    detail_view: DetailView,
    browser_visible: bool,
    browser_width: f32,
    detail_height_ratio: f32,  // 0.0–1.0
}
```

---

## 4. Control Bar

### Layout paska transportu

Pasek o stałej wysokości 40px, zawsze na górze. Podzielony na logiczne sekcje:

```
┌──────────────────────────────────────────────────────────────────┐
│ [≡] │ ⏮ ⏪  ⏵  ⏹  ⏺ │ 120.00 BPM  4/4 │ 1.1.1 │ 🔁 Loop │ CPU 12% │
│ Menu│    Transport     │  Tempo  │ Metrum│Pozycja│  Loop   │ Monitor │
└──────────────────────────────────────────────────────────────────┘
```

### Elementy Control Bar

**Sekcja Transport:**
- **Play/Pause** (`Space`) — ikona ▶/⏸, podświetlenie zielone gdy gra
- **Stop** (`Space` gdy gra, lub dedykowany przycisk) — wraca do pozycji startu
- **Record** (`R`) — czerwona kropka, pulsuje gdy nagrywa, arm dla audio/MIDI
- **Skip back** (`Home`) — wraca na początek projektu
- **Rewind/Forward** — przewijanie ze stałą prędkością

**Sekcja Tempo:**
- **BPM display** — edytowalne pole numeryczne, klik + drag góra/dół zmienia wartość (zakres 20–999 BPM, precyzja 0.01)
- **Time signature** — dropdown `4/4`, `3/4`, `6/8` itd.
- **Tap tempo** — przycisk lub `T`, oblicza BPM z interwałów kliknięć
- **Metronom toggle** (`M`) — ikona kliknięcia, opcje: głośność, pre-count barów

**Sekcja pozycji:**
- **Position display** — format `bars.beats.sixteenths` (np. `12.3.2`)
- **Time display** — alternatywnie `mm:ss.ms` (przełączane kliknięciem)
- **Follow playhead** (`F`) — scroll podąża za pozycją odtwarzania

**Sekcja Loop:**
- **Loop toggle** (`L`) — aktywuje/dezaktywuje pętlę
- **Loop start/end** — edytowalne numerycznie lub przeciągane na timelinie
- **Punch in/out** — dla nagrywania w określonym zakresie

**Sekcja Monitor:**
- **CPU meter** — pasek procentowy obciążenia audio thread
- **Buffer underrun indicator** — czerwona lampka przy dropoutach
- **Sample rate display** — `44100 Hz` / `48000 Hz`
- **MIDI activity indicator** — migająca lampka przy sygnale MIDI in

### Implementacja w vizia

```rust
// Pseudo-kod struktury danych
#[derive(Lens)]
struct TransportState {
    playing: bool,
    recording: bool,
    bpm: f64,
    time_sig_num: u8,
    time_sig_den: u8,
    position_beats: f64,
    loop_enabled: bool,
    loop_start: f64,  // w beatach
    loop_end: f64,
    metronome_on: bool,
    cpu_load: f32,
    follow_playhead: bool,
}
```

---

## 5. Arrangement View

### Struktura widoku

Arrangement View to liniowa oś czasu — główny widok do aranżacji utworu. Ścieżki ułożone pionowo (od góry do dołu), czas biegnie od lewej do prawej.

```
┌─────────────┬──────────────────────────────────────────────┐
│             │  1    2    3    4    5    6    7    8    9    │ ← Ruler (takt/beat)
│             ├──────────────────────────────────────────────┤
│             │▊▊▊▊▊▊▊▊▊▊▊▊▊▊▊▊▊▊▊│                       │ ← Loop region (żółty)
│             ├──────────────────────────────────────────────┤
│ Track 1     │ ┌────────────┐  ┌─────────────────────┐     │
│ "Kick"      │ │ ~~waveform~│  │  ~~waveform~~~~~~~~ │     │ ← Audio clips
│ 🔇 S M     │ └────────────┘  └─────────────────────┘     │
├─────────────┼──────────────────────────────────────────────┤
│ Track 2     │ ┌──────────────────┐  ┌───────────┐         │
│ "Bass"      │ │ ▪▪▪▪  ▪▪▪▪▪▪▪▪ │  │ ▪▪▪  ▪▪▪ │         │ ← MIDI clips
│ 🔇 S M     │ └──────────────────┘  └───────────┘         │
├─────────────┼──────────────────────────────────────────────┤
│ Track 3     │                                              │
│ "Vocals"    │ (pusta ścieżka — gotowa do nagrywania)      │
│ 🔇 S M ⏺   │                                              │
└─────────────┴──────────────────────────────────────────────┘
                ▲ Playhead (czerwona pionowa linia)
```

### Track Header (panel nagłówka ścieżki)

Stała szerokość ~180px po lewej stronie. Każdy nagłówek zawiera:

```
┌─────────────────────────┐
│ 🎵 Track 1 - "Kick"    │  ← Nazwa (edytowalna double-click)
│ ┌──┐ ┌──┐ ┌──┐  ┌───┐  │
│ │🔇│ │ S│ │ M│  │⏺ │  │  ← Mute, Solo, Monitor, Record Arm
│ └──┘ └──┘ └──┘  └───┘  │
│ ▓▓▓▓▓▓▓▓░░░░  -6.2 dB  │  ← Mini volume slider
│ In: M-Audio 1 │Out: Mstr│  ← I/O routing (dropdowny)
└─────────────────────────┘
```

**Elementy nagłówka:**
- **Kolor ścieżki** — pasek kolorowy na lewej krawędzi (prawy klik → paleta 16 kolorów)
- **Ikona typu** — 🎵 audio, 🎹 MIDI, 📁 grupa, 🔊 return/bus
- **Nazwa** — edytowalna inline (`F2` lub double-click)
- **Mute** (`Ctrl+M`) — wycisza ścieżkę, dimmed clip visuals
- **Solo** (`Ctrl+S`) — soluje ścieżkę, reszta wyciszona
- **Record Arm** (`Ctrl+R`) — aktywuje nagrywanie na tej ścieżce
- **Volume** — mini slider z wartością dB (range: -inf do +6 dB)
- **I/O routing** — dropdown source (hardware input / none) i output (master / bus / hardware)
- **Fold/Unfold** — zwijanie zawartości ścieżki (strzałka ▼/▶)

### Ruler (linijka czasowa)

Pasek nad ścieżkami wyświetlający pozycję w taktach i beatach:

- **Zoom-dependent labels** — przy dużym zoomie: takty + beaty, przy małym: co 4/8/16 taktów
- **Snap grid** — linie siatki widoczne przez cały timeline (przezroczyste, ~10% opacity)
- **Klik na ruler** = przesuń playhead do tej pozycji
- **Drag na ruler** = scrub audio (odtwarzanie z pozycji kursora)
- **Loop bracket** — żółty/pomarańczowy prostokąt na rulerze definiujący region pętli

### Clip rendering

**Audio clip:**
- Prostokąt z zaokrąglonymi rogami (radius 4px)
- Kolor tła = kolor ścieżki (lekko jaśniejszy)
- Nazwa clipu w lewym górnym rogu (8px font, biały z cieniem)
- Waveform wewnątrz (patrz sekcja 11 — Renderowanie waveform)
- Fade in/out — trójkątne uchwyty na rogach clipu

**MIDI clip:**
- Prostokąt z kolorem ścieżki
- Miniaturowe nuty widoczne jako małe prostokąty (wysokość = pitch, długość = czas)
- Kolory nut mapowane na velocity (ciemne = cicho, jasne = głośno)

### Nawigacja i zoom

- **Scroll poziomy** — `Scroll` na timelinie lub `Shift+Scroll`
- **Scroll pionowy** — `Scroll` na track headerach
- **Zoom poziomy** — `Ctrl+Scroll` lub `+`/`-` (center na kursorze myszy)
- **Zoom pionowy** — `Ctrl+Shift+Scroll` (zmienia wysokość ścieżek)
- **Zoom to selection** — `Z` zoomuje na zaznaczony region
- **Zoom to fit** — `Ctrl+Shift+F` pokazuje cały projekt
- **Overview bar** — miniatura całego projektu na górze, klikalne do nawigacji

### Zaznaczanie i edycja

- **Click** na clip = zaznacz (niebieska obramówka)
- **Click + drag** na pustym = rubber band selection
- **Ctrl+Click** = dodaj do zaznaczenia
- **Drag clip** = przesuń (snap do grid)
- **Ctrl+Drag** = przesuń bez snap
- **Ctrl+D** = duplikuj clip
- **Delete** = usuń zaznaczone
- **Ctrl+E** = podziel clip w pozycji playhead
- **Shift+Drag krawędzi** = resize clip (trim audio / zmień długość MIDI)
- **Double-click na clip** = otwórz w Detail View (piano roll lub audio editor)

---

## 6. Session View

### Struktura siatki klipów

Session View to siatka klipów do improwizacji i występów na żywo. Kolumny = ścieżki, wiersze = sceny.

```
┌───────────┬───────────┬───────────┬───────────┬──────────┐
│  Scene 1  │           │           │           │  ▶ Scn 1 │ ← Scene launch
├───────────┼───────────┼───────────┼───────────┤──────────┤
│ ▶ Clip A  │ ▶ Clip D  │ ▶ Clip G  │           │  ▶ Scn 2 │
│ ~~wave~~  │ ▪▪ ▪▪▪▪  │ ~~wave~~  │  [empty]  │          │
├───────────┼───────────┼───────────┼───────────┤──────────┤
│ ▶ Clip B  │ ▶ Clip E  │           │ ▶ Clip J  │  ▶ Scn 3 │
│ ~~wave~~  │ ▪▪▪  ▪▪  │  [empty]  │ ▪▪▪▪ ▪▪  │          │
├───────────┼───────────┼───────────┼───────────┤──────────┤
│ ■ Stop    │ ■ Stop    │ ■ Stop    │ ■ Stop    │  ■ Stop  │
├───────────┼───────────┼───────────┼───────────┤──────────┤
│  Kick     │  Bass     │  Pads     │  Vocals   │  Master  │
│ 🔇 S ⏺    │ 🔇 S ⏺    │ 🔇 S ⏺    │ 🔇 S ⏺    │          │
│ ▓▓▓░░ Vol │ ▓▓░░░ Vol │ ▓▓▓▓░ Vol │ ▓░░░░ Vol │ ▓▓▓░ Vol │
└───────────┴───────────┴───────────┴───────────┴──────────┘
```

### Clip Slot

Każdy slot (komórka siatki) ma trzy stany:

- **Pusty** — ciemnoszare tło, drop target dla plików audio/MIDI
- **Załadowany, zatrzymany** — miniatura clipu (waveform lub nuty), przycisk ▶ na hover
- **Odtwarzany** — zielona obramówka, pulsujący przycisk ▶, pasek postępu na dole

**Interakcje ze slotem:**
- **Klik ▶** — launch clip (odtwórz)
- **Klik ■** — stop clip na tej ścieżce
- **Double-click** — otwórz w Detail View
- **Drag & drop** pliku audio/MIDI — załaduj do slotu
- **Prawy klik** — context menu (rename, color, quantize, delete)

### Scene launch

Kolumna Master po prawej stronie zawiera przyciski Scene Launch — uruchamiają wszystkie clipy w danym wierszu jednocześnie. Pozwala to na przełączanie "sekcji" utworu (verse, chorus, bridge) jednym kliknięciem.

### Clip launch quantization

Dropdown w Control Bar kontroluje kiedy clip faktycznie startuje po kliknięciu:
- **None** — natychmiast
- **1 bar** — na następnym takcie (domyślne)
- **1/2, 1/4, 1/8** — na następnym beacie podanej wartości

---

## 7. Mixer

### Layout miksera

Mikser może być wyświetlany jako:
1. **Inline** w dolnej części Arrangement/Session View
2. **Full-screen** (przełączany `Ctrl+Shift+M`)

```
┌────────┬────────┬────────┬────────┬────────┬─────────┐
│Track 1 │Track 2 │Track 3 │Track 4 │Return A│ MASTER  │
│ "Kick" │ "Bass" │ "Pads" │"Vocals"│"Reverb"│         │
├────────┼────────┼────────┼────────┼────────┼─────────┤
│ [Send] │ [Send] │ [Send] │ [Send] │        │         │
│  A: -12│  A: -18│  A: -6 │  A: -24│        │         │
│  B: off│  B: -12│  B: off│  B: -6 │        │         │
├────────┼────────┼────────┼────────┼────────┼─────────┤
│  ◀PAN▶ │  ◀PAN▶ │  ◀PAN▶ │  ◀PAN▶ │  ◀PAN▶ │  ◀PAN▶  │
├────────┼────────┼────────┼────────┼────────┼─────────┤
│ ┃▓▓▓▓┃ │ ┃▓▓▓ ┃ │ ┃▓▓   ┃ │ ┃▓    ┃ │ ┃▓▓  ┃ │ ┃▓▓▓▓▓┃ │ ← Volume faders
│ ┃▓▓▓▓┃ │ ┃▓▓▓ ┃ │ ┃▓▓   ┃ │ ┃▓    ┃ │ ┃▓▓  ┃ │ ┃▓▓▓▓▓┃ │
│ ┃▓▓▓▓┃ │ ┃▓▓▓ ┃ │ ┃▓▓   ┃ │ ┃▓    ┃ │ ┃▓▓  ┃ │ ┃▓▓▓▓▓┃ │
│ -2.4dB │ -6.0dB│ -12dB  │ -18dB  │ -8.0dB│  0.0dB  │
├────────┼────────┼────────┼────────┼────────┼─────────┤
│ 🔇 S ⏺ │ 🔇 S ⏺ │ 🔇 S ⏺ │ 🔇 S ⏺ │ 🔇 S   │ 🔇      │
├────────┼────────┼────────┼────────┼────────┼─────────┤
│[▓▓|▓▓]│[▓▓|▓ ]│[▓ |▓ ]│[▓ |  ]│[▓▓|▓▓]│[▓▓▓|▓▓▓]│ ← VU meters (stereo)
└────────┴────────┴────────┴────────┴────────┴─────────┘
```

### Mixer Channel Strip — elementy

Od góry do dołu, każdy kanał zawiera:

1. **Nazwa ścieżki** — edytowalna, kolor background = kolor ścieżki
2. **I/O selector** — dropdown wejścia i wyjścia
3. **Device slots** — miniaturowe ikony załadowanych efektów (klik = otwórz w Device Rack)
4. **Send knobs** — obrotowe pokrętła send do return tracków (A, B, C, D)
5. **Pan knob** — panorama L/R (-100 do +100)
6. **Volume fader** — pionowy suwak, zakres -inf do +6dB, skala logarytmiczna
7. **dB display** — aktualna wartość faderu numerycznie
8. **Mute/Solo/Arm** — przyciski
9. **VU meter** — stereo peak meter z hold indicator (patrz sekcja 12)

### VU Meter — specyfikacja

Każdy VU meter to para pionowych pasków (L/R) z następującą skalą kolorów:

- **-inf do -18 dB** — zielony (#4CAF50)
- **-18 do -6 dB** — żółto-zielony gradient
- **-6 do -3 dB** — żółty (#FFC107)
- **-3 do 0 dB** — pomarańczowy (#FF9800)
- **0 dB+** — czerwony (#F44336) — clip indicator

Parametry renderowania:
- **Refresh rate** — 60fps (synchronizowane z vsync)
- **Peak hold** — biała linia peak trzymana 2 sekundy, potem spada 20dB/s
- **Ballistics** — attack ~5ms (prawie natychmiastowy), release ~300ms (płynne opadanie)
- **RMS overlay** (opcjonalny) — ciemniejszy pasek wewnątrz peak metera

---

## 8. Piano Roll

### Widok ogólny

Piano Roll to najważniejszy edytor MIDI. Otwiera się w Detail View po double-clicku na MIDI clip.

```
┌────┬────────────────────────────────────────────────┐
│    │  1.1  1.2  1.3  1.4  2.1  2.2  2.3  2.4      │ ← Beat ruler
├────┼────────────────────────────────────────────────┤
│ C5 │         ┌──────┐                               │
│ B4 │                                                │
│ A4 │   ┌──────────────────┐                         │
│ G4 │                          ┌────┐                │ ← Nuty MIDI
│ F4 │                                                │   (prostokąty)
│ E4 │              ┌──────────────┐                  │
│ D4 │                                                │
│ C4 │ ┌────┐                         ┌──────────┐   │
│ B3 │                                                │
│ A3 │                                                │
├────┼────────────────────────────────────────────────┤
│    │ ▓▓▓ ▓▓  ▓▓▓▓▓ ▓▓  ▓▓  ▓▓▓▓ ▓▓▓  ▓▓  ▓▓▓▓   │ ← Velocity lane
│    │ 100  80  127   60  90  100  75   85  110       │
└────┴────────────────────────────────────────────────┘
```

### Klawiatura piano (lewa strona)

- Klawisze białe i czarne renderowane jak klawiatura fortepianu (pionowo)
- **Klik na klawisz** = preview dźwięku (wysyła MIDI note do aktywnego instrumentu)
- **Zakres** — C-2 do G8 (standard MIDI 0–127)
- **Scroll** — drag na klawiaturze lub scroll wheel
- **Highlight aktywnego octave** — C4 = Middle C, wyróżnione

### Siatka nut

- **Tło** — alternujące jasne/ciemne pasy co oktawę. Czarne klawisze = ciemniejsze tło
- **Grid lines** — pionowe linie w zależności od snap:
  - 1/4 note: linie na każdym beacie
  - 1/8: linie co pół beata
  - 1/16: linie co ćwierć beata
  - 1/32: najdrobniejsza siatka
  - Triplet variants: 1/4T, 1/8T, 1/16T
- **Snap toggle** — `Ctrl+G` włącza/wyłącza przyciąganie do siatki

### Nuty MIDI — renderowanie

Każda nuta to prostokąt:

```
┌─────────────────────────────┐
│ C4          vel: 100        │  ← Tekst widoczny przy dużym zoomie
└─────────────────────────────┘
  ↑ pozycja Y = pitch           ↑ szerokość = duration
```

- **Kolor** — mapowany na velocity: gradient od ciemnoniebieskiego (vel 1) do jasnopomarańczowego (vel 127)
- **Zaznaczona nuta** — biała obramówka + jasny overlay
- **Hover** — tooltip z pitch, velocity, position, duration

### Edycja nut

- **Klik na pustym** = wstaw nową nutę (długość = aktualny snap)
- **Klik na nucie** = zaznacz
- **Drag nuta** = przesuń (pitch + czas, snap do gridu)
- **Drag prawy edge** = zmień długość
- **Ctrl+Click** = dodaj do zaznaczenia
- **Shift+Drag** = rubber band selection
- **Ctrl+A** = zaznacz wszystkie
- **Ctrl+D** = duplikuj zaznaczone
- **Delete** = usuń zaznaczone
- **↑/↓** = transpozycja o półton
- **Shift+↑/↓** = transpozycja o oktawę
- **Ctrl+Q** = quantize zaznaczone (snap do najbliższego grid point)

### Velocity Lane (dolny pas)

Pod siatką nut, pasek ~60px wysokości:

- Każda nuta reprezentowana jako pionowy słupek
- Wysokość słupka = velocity (0–127)
- **Drag górnej krawędzi** słupka = zmień velocity
- **Draw tool** — rysuj velocity linią/krzywą (przytrzymanie Ctrl)
- **Multi-select velocity** — zaznacz nuty → drag zmienia wszystkie proporcjonalnie

### Narzędzia Piano Roll

Toolbar nad piano rollem:

- **Pointer tool** (V) — zaznaczanie i przesuwanie
- **Draw tool** (D) — rysowanie nowych nut kliknięciem
- **Erase tool** (E) — kasowanie nut kliknięciem
- **Snap selector** — dropdown: off, 1/4, 1/8, 1/16, 1/32, triplet warianty
- **Length selector** — domyślna długość nowej nuty
- **Scale highlight** — podświetl nuty w wybranej skali (np. C major, A minor)

### Struktura danych Piano Roll

```rust
/// Pojedyncza nuta w piano rollu
struct PianoRollNote {
    id: NoteId,
    pitch: u8,           // 0-127, MIDI note number
    velocity: u8,        // 0-127
    start_tick: u64,     // pozycja startowa w PPQ ticks
    duration_ticks: u64, // długość w PPQ ticks
    channel: u8,         // 0-15
    selected: bool,
    muted: bool,
}

/// Stan piano rollu
struct PianoRollState {
    notes: Vec<PianoRollNote>,
    ppq: u32,            // 480 lub 960
    snap_value: SnapValue,
    zoom_x: f64,         // piksele na tick
    zoom_y: f64,         // piksele na półton
    scroll_x: f64,       // offset w tickach
    scroll_y: f64,       // offset w półtonach
    tool: PianoRollTool,
    velocity_lane_visible: bool,
}

enum SnapValue {
    Off,
    Bar,
    HalfBar,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    QuarterTriplet,
    EighthTriplet,
    SixteenthTriplet,
}
```

---

## 9. Device Rack

### Koncept

Device Rack to łańcuch instrumentów i efektów audio na danej ścieżce. Sygnał przepływa od lewej do prawej. Wyświetlany w Detail View na dole ekranu.

```
┌─────────────────────────────────────────────────────────────────┐
│ Track: "Bass" │ 🔌 Add Device                                   │
├────────────────┬────────────────┬────────────────┬──────────────┤
│ 🎹 FundSynth   │ 🎛️ EQ 3-Band   │ 🎛️ Compressor  │ 🎛️ Reverb    │
│                │                │                │              │
│ ┌──┐ ┌──┐     │  Lo  Mid  Hi   │ Thresh  Ratio  │ Size  Decay  │
│ │🔘│ │🔘│     │ ┌──┐┌──┐┌──┐  │  ┌──┐   ┌──┐   │ ┌──┐  ┌──┐  │
│ │  │ │  │     │ │🔘││🔘││🔘│  │  │🔘│   │🔘│   │ │🔘│  │🔘│  │
│ │  │ │  │     │ │  ││  ││  │  │  │  │   │  │   │ │  │  │  │  │
│ └──┘ └──┘     │ └──┘└──┘└──┘  │  └──┘   └──┘   │ └──┘  └──┘  │
│ Freq   Res    │               │  Attack Release │ Mix   Width  │
│               │               │                │              │
│ [🔇][bypass]  │ [🔇][bypass]  │ [🔇][bypass]   │ [🔇][bypass] │
└────────────────┴────────────────┴────────────────┴──────────────┘
  ← sygnał płynie od lewej do prawej →
```

### Elementy Device

Każde urządzenie (device) w łańcuchu to panel ~150–200px szerokości:

- **Header** — nazwa urządzenia, ikona typu (instrument/efekt), przycisk fold (zwija do ikony)
- **Knobs** — obrotowe pokrętła parametrów (patrz sekcja 12)
- **Bypass toggle** — wyłącza device z łańcucha (przycisk lub klik na header)
- **Mute** — wycisza output device'a
- **Drag handle** — przeciąganie zmienia kolejność w łańcuchu
- **Remove** — `Delete` lub prawy klik → Remove

### Typy Device'ów

**Instrumenty (wbudowane, fundsp):**
- **FundSynth** — subtraktywny synth: 2 oscylatory (saw/square/sine/tri), filtr LP/HP/BP, ADSR, LFO
- **SamplePlayer** — odtwarzacz sampli z ADSR i filtrami
- **DrumMachine** — 16 padów, każdy z własnym samplem i ADSR

**Efekty (wbudowane, fundsp):**
- **EQ 3-Band** — Lo/Mid/Hi z frequency i gain knobami
- **Compressor** — threshold, ratio, attack, release, makeup gain
- **Reverb** — room size, decay, damping, dry/wet
- **Delay** — time (sync to BPM), feedback, dry/wet
- **Chorus** — rate, depth, mix
- **Distortion** — drive, tone, mix

**Pluginy zewnętrzne (CLAP via clack):**
- Skanowanie folderów pluginów
- Wyświetlanie nazwy + GUI pluginu w osobnym oknie (baseview)
- Parametry pluginu mapowane na generyczne knobs w rack

### Drag & Drop

- **Przesuń device** — drag w ramach łańcucha zmienia pozycję
- **Dodaj device** — drag z Browsera lub przycisk "Add Device"
- **Kopiuj device** — `Alt+Drag` tworzy kopię z tymi samymi ustawieniami

---

## 10. Browser

### Struktura

Panel po lewej stronie (~250px) z trzema zakładkami:

```
┌─────────────────────────────┐
│ [Files] [Devices] [Presets] │ ← Zakładki
├─────────────────────────────┤
│ 🔍 Search...                │ ← Pole wyszukiwania
├─────────────────────────────┤
│ 📁 Project Files            │
│   📁 Samples                │
│     🎵 kick.wav             │
│     🎵 snare.wav            │
│   📁 MIDI                   │
│     🎹 bass_line.mid        │
│ 📁 User Library             │
│   📁 My Samples             │
│   📁 My Presets             │
│ 📁 System Library           │
│   📁 Factory Sounds         │
├─────────────────────────────┤
│ ▶ Preview: kick.wav         │ ← Preview player
│ ▓▓▓▓▓▓▓░░░░░░░░ 0:02/0:05 │
└─────────────────────────────┘
```

### Zakładki

- **Files** — drzewo systemu plików z foldery projektu, user library, factory sounds
- **Devices** — lista wbudowanych instrumentów i efektów + zainstalowane pluginy CLAP
- **Presets** — presety dla wybranego device'a

### Preview

Na dole Browsera, mini-player z waveformem:
- **Klik na pliku audio** = automatyczny preview (toggle w settings)
- **Klik na pliku MIDI** = preview przez aktualnie zaznaczony instrument
- **Volume knob** — głośność preview
- **Stop** — zatrzymaj preview

### Drag & Drop z Browsera

- **Audio file → Arrangement** = tworzy audio clip na ścieżce
- **Audio file → Session slot** = ładuje clip do slotu
- **MIDI file → track** = tworzy MIDI clip
- **Device → track** = dodaje do łańcucha efektów
- **Audio file → Session (pusta kolumna)** = tworzy nową ścieżkę

---

## 11. Renderowanie waveform

### System wielopoziomowego cache'u (mipmap peaks)

Renderowanie waveform jest najcięższą operacją graficzną w DAW. Plik audio o długości 5 minut przy 44.1kHz to **13.2 miliona sampli** — nie można rysować każdego z nich.

Rozwiązanie: **hierarchiczny cache peaków** (peak mipmap), analogiczny do mipmapowania tekstur w grafice 3D.

```
Poziom 0: surowe sample (44100/s)
Poziom 1: min/max co 64 sample (~689/s)
Poziom 2: min/max co 256 sampli (~172/s)
Poziom 3: min/max co 1024 sample (~43/s)
Poziom 4: min/max co 4096 sampli (~11/s)
Poziom 5: min/max co 16384 sample (~3/s)
```

Każdy poziom cache'u przechowuje parę `(min, max)` dla każdego bloku. Przy renderowaniu wybieramy poziom najbliższy aktualnemu zoomowi — tak aby ~1 piksel odpowiadał ~1 blokowi peak cache.

### Struktura danych peak cache

```rust
struct PeakCache {
    /// Cache hierarchiczny — levels[0] = najdrobniejszy
    levels: Vec<PeakLevel>,
    sample_rate: u32,
    channels: u16,
}

struct PeakLevel {
    block_size: u32,           // 64, 256, 1024, 4096, 16384
    peaks: Vec<PeakPair>,      // interleaved channels
}

#[derive(Copy, Clone)]
struct PeakPair {
    min: f32,   // minimum sample w bloku
    max: f32,   // maximum sample w bloku
}
```

### Algorytm renderowania

```
Dla każdego piksela kolumny na ekranie:
  1. Oblicz zakres sampli odpowiadający temu pikselowi:
     sample_start = pixel_x * samples_per_pixel + scroll_offset
     sample_end   = sample_start + samples_per_pixel

  2. Wybierz poziom cache gdzie block_size ≈ samples_per_pixel

  3. Odczytaj min/max z odpowiednich bloków cache

  4. Narysuj pionową linię od min do max (skalowane do wysokości clipu)
```

### Rendering na Skia Canvas

```rust
fn draw_waveform(
    canvas: &mut Canvas,
    peaks: &PeakLevel,
    rect: Rect,           // prostokąt clipu na ekranie
    color: Color,
    start_sample: u64,
    samples_per_pixel: f64,
) {
    let center_y = rect.center_y();
    let half_height = rect.height() / 2.0;

    for px in 0..rect.width() as usize {
        let block_idx = ((start_sample as f64 + px as f64 * samples_per_pixel)
            / peaks.block_size as f64) as usize;

        if block_idx < peaks.peaks.len() {
            let peak = peaks.peaks[block_idx];
            let y_min = center_y - peak.max * half_height;  // max = top
            let y_max = center_y - peak.min * half_height;  // min = bottom

            canvas.draw_line(
                (rect.left + px as f32, y_min),
                (rect.left + px as f32, y_max),
                &color_paint,
            );
        }
    }
}
```

### Budowanie cache'u

Peak cache budowany jest **w osobnym wątku** przy importowaniu pliku audio:

1. Wczytaj cały plik przez symphonia → `Vec<f32>`
2. Buduj level 0: iteruj co 64 sample, zapisz min/max
3. Buduj level 1: iteruj co 4 bloki z level 0, zapisz min/max
4. (itd. dla każdego poziomu)
5. Zapisz cache do pliku `.peaks` obok pliku audio (dla szybkiego ponownego wczytania)

Czas budowania: ~200ms dla 5-minutowego pliku stereo (jednowątkowe). Waveform jest widoczny natychmiast na najgrubszym dostępnym poziomie, a dokładniejsze poziomy pojawiają się progresywnie.

---

## 12. Widgety audio-specyficzne

### Knob (obrotowe pokrętło)

Najważniejszy widget w DAW — kontroluje każdy parametr.

```
      ╭─────╮
     ╱   ╱   ╲        ← Łuk "track" (szary = tło, kolorowy = wartość)
    │   ╱     │
    │  ╱  ●   │        ← Wskaźnik pozycji (biała kropka/linia)
    │ ╱       │
     ╲       ╱
      ╰─────╯
     Freq: 440 Hz      ← Label z wartością
```

**Interakcja:**
- **Drag pionowy** — zmienia wartość (góra = więcej, dół = mniej)
- **Ctrl+Drag** — fine-tuning (10x wolniejsza zmiana)
- **Double-click** — reset do default
- **Prawy klik** — context menu (type value, MIDI learn, reset)
- **Scroll wheel** — zmienia wartość krok po kroku

**Parametry renderowania:**
- Rozmiar: 48×48px (standard), 32×32px (compact), 64×64px (large)
- Arc track: 270° sweep (od 7 o'clock do 5 o'clock)
- Value arc: kolorowy od startu do aktualnej pozycji
- Pointer: biała linia od centrum do krawędzi

**Krytyczna cecha:** **pointer locking** — kursor chowa się podczas drag i reappearuje na pozycji startu po zwolnieniu. Bez tego knob przestaje działać gdy kursor opuści okno (problem GTK identyfikowany przez Meadowlark).

### Fader (suwak)

Pionowy suwak do kontroli głośności.

- **Skala logarytmiczna** — dB scale, -inf na dole, +6dB na górze
- **Markery** — linie co 6dB, label co 12dB
- **Thumb** — przeciągalny uchwyt ~20px
- **Double-click** = reset do 0 dB
- **Ctrl+Drag** = fine-tuning

### Peak Meter (wskaźnik poziomu)

Stereo peak meter z następującymi cechami:

- **Segmented rendering** — 2 kolumny (L/R), każda podzielona na ~50 segmentów
- **Color gradient** — zielony → żółty → pomarańczowy → czerwony (patrz sekcja 7)
- **Peak hold** — biała linia na najwyższym piku, trzymana 2s, potem spada 20dB/s
- **Clip indicator** — czerwony prostokąt na samej górze, zapala się przy clippingu, resetowany kliknięciem
- **RMS overlay** (opcjonalnie) — ciemniejszy pasek pod peakiem pokazujący RMS level
- **Refresh 60fps** — dane z audio thread przez `triple_buffer`

### XY Pad

Kontroler dwuparametrowy (np. filter cutoff + resonance):

- Kwadratowy area 120×120px
- Punkt (kółko 8px) wskazuje pozycję obu parametrów
- Drag = zmień oba parametry jednocześnie
- Osie X i Y z labelami parametrów

---

## 13. Komunikacja GUI ↔ Audio Thread

### Architektura komunikacji

```
┌─────────────┐     rtrb (lock-free)      ┌─────────────────┐
│             │ ◄──── Parameter changes ────│                 │
│ AUDIO       │                            │  GUI THREAD     │
│ THREAD      │ ───── Meter data ────────► │  (60fps)        │
│ (realtime)  │ ───── Playhead pos ──────► │                 │
│             │ ───── MIDI activity ──────►│                 │
└─────────────┘                            └─────────────────┘
       │                                          │
       │ rtrb                              rtrb   │
       ▼                                          ▼
┌─────────────┐                            ┌─────────────────┐
│ DISK I/O    │                            │ COMMAND          │
│ THREAD      │                            │ PROCESSOR        │
│ (streaming) │                            │ (add/remove trk) │
└─────────────┘                            └─────────────────┘
```

### Typy wiadomości

```rust
/// GUI → Audio Thread (przez rtrb)
enum AudioCommand {
    SetParameter { track_id: u32, device_id: u32, param_id: u32, value: f32 },
    Transport(TransportCommand),
    SetTrackVolume { track_id: u32, volume: f32 },
    SetTrackPan { track_id: u32, pan: f32 },
    MuteTrack { track_id: u32, muted: bool },
    SoloTrack { track_id: u32, solo: bool },
}

enum TransportCommand {
    Play,
    Stop,
    Record,
    Seek(u64),           // sample position
    SetBpm(f64),
    SetLoop { start: u64, end: u64, enabled: bool },
}

/// Audio Thread → GUI (przez triple_buffer lub rtrb)
struct AudioFeedback {
    playhead_samples: u64,
    track_meters: Vec<StereoMeter>,  // peak + RMS per track
    master_meter: StereoMeter,
    cpu_load: f32,
    midi_activity: bool,
    buffer_underrun: bool,
}

struct StereoMeter {
    peak_l: f32,
    peak_r: f32,
    rms_l: f32,
    rms_r: f32,
}
```

### Zasada: GUI nigdy nie blokuje Audio

- GUI **odczytuje** stan audio przez `triple_buffer` (zawsze widzi najnowszą wartość)
- GUI **wysyła** komendy przez `rtrb` ring buffer (SPSC, wait-free)
- Audio thread **nigdy** nie czeka na GUI — jeśli ring buffer pełny, komenda jest dropped
- Parametry interpolowane z smoothing (audio thread robi ramping, nie skokowe zmiany)

---

## 14. System kolorów i theming

### Paleta bazowa (Dark Theme)

```
Background (główne tło):          #1E1E1E
Surface (panele, headery):        #2D2D2D
Surface elevated (popup, tooltip): #3D3D3D
Border (ramki paneli):            #404040
Text primary (biały):             #E0E0E0
Text secondary (szary):           #A0A0A0
Text disabled:                    #606060

Accent primary (focus, selection): #5B9BD5
Accent secondary:                 #7BC47F

Playhead:                         #FF4444
Recording:                        #FF3333 (pulsujące)
Loop region:                      #FFB74D (30% opacity)
Grid lines:                       #FFFFFF (8% opacity)

Meter green:                      #4CAF50
Meter yellow:                     #FFC107
Meter orange:                     #FF9800
Meter red (clip):                 #F44336
```

### Kolory ścieżek (paleta 16)

```
#E57373  #F06292  #BA68C8  #9575CD
#7986CB  #64B5F6  #4FC3F7  #4DD0E1
#4DB6AC  #81C784  #AED581  #DCE775
#FFD54F  #FFB74D  #FF8A65  #A1887F
```

### Velocity color gradient (piano roll)

```
Velocity 1:   #1A237E (ciemny niebieski)
Velocity 32:  #1565C0
Velocity 64:  #42A5F5
Velocity 96:  #FFA726 (pomarańczowy)
Velocity 127: #FF6F00 (jasny pomarańczowy)
```

### CSS theming (vizia)

```css
/* theme.css — hot-reloadable */
:root {
    --bg-primary: #1E1E1E;
    --bg-surface: #2D2D2D;
    --bg-elevated: #3D3D3D;
    --border: #404040;
    --text-primary: #E0E0E0;
    --text-secondary: #A0A0A0;
    --accent: #5B9BD5;
    --playhead: #FF4444;
}

.track-header {
    background-color: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    height: 80px;
}

knob {
    width: 48px;
    height: 48px;
}

knob .track {
    background-color: var(--accent);
}
```

---

## 15. Skróty klawiszowe i workflow

### Globalne

| Skrót | Akcja |
|-------|-------|
| `Space` | Play / Stop |
| `R` | Record toggle |
| `Tab` | Przełącz Arrangement ↔ Session View |
| `L` | Loop on/off |
| `M` | Metronom on/off |
| `F` | Follow playhead |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` | Redo |
| `Ctrl+S` | Zapisz projekt |
| `Ctrl+N` | Nowy projekt |
| `Ctrl+O` | Otwórz projekt |
| `Ctrl+Alt+B` | Pokaż/ukryj Browser |
| `Ctrl+Shift+M` | Pokaż/ukryj Mixer (full-screen) |

### Arrangement View

| Skrót | Akcja |
|-------|-------|
| `Ctrl+T` | Nowa ścieżka audio |
| `Ctrl+Shift+T` | Nowa ścieżka MIDI |
| `Ctrl+E` | Podziel clip na pozycji playhead |
| `Ctrl+J` | Połącz zaznaczone clipy |
| `Ctrl+D` | Duplikuj zaznaczenie |
| `Ctrl+A` | Zaznacz wszystko |
| `Delete` | Usuń zaznaczone |
| `Z` | Zoom do zaznaczenia |
| `Ctrl+Shift+F` | Zoom to fit (cały projekt) |
| `+` / `-` | Zoom in / out |
| `Home` | Skocz na początek |
| `End` | Skocz na koniec |

### Piano Roll

| Skrót | Akcja |
|-------|-------|
| `V` | Pointer tool |
| `D` | Draw tool |
| `E` | Erase tool |
| `Ctrl+Q` | Quantize zaznaczone |
| `↑` / `↓` | Transpozycja ±1 półton |
| `Shift+↑/↓` | Transpozycja ±1 oktawa |
| `Ctrl+G` | Snap on/off |
| `Ctrl+A` | Zaznacz wszystkie nuty |

---

## 16. Roadmapa implementacji

### Faza 1 — Fundament (4-6 tygodni)

**Cel: odtwarzanie i nagrywanie audio z GUI**

- [ ] Okno główne vizia z Control Bar (play/stop/record, BPM)
- [ ] Arrangement View — statyczna lista ścieżek z track headerami
- [ ] Waveform rendering (peak cache, single level)
- [ ] Audio playback przez cpal (ASIO na Windows, ALSA na Linux)
- [ ] Nagrywanie mono/stereo na ścieżkę (ring buffer → hound WAV)
- [ ] Transport: play, stop, seek (click na ruler)
- [ ] Komunikacja GUI ↔ Audio przez rtrb + triple_buffer
- [ ] VU metery na master channel

### Faza 2 — MIDI & Piano Roll (4-6 tygodni)

**Cel: tworzenie i edycja MIDI**

- [ ] Piano Roll — siatka, rysowanie nut, velocity lane
- [ ] Wbudowany synth (fundsp: subtractive, ADSR)
- [ ] MIDI input z kontrolera (midir)
- [ ] MIDI clip playback zsynchronizowany z transport
- [ ] Quantize
- [ ] Snap grid z wieloma wartościami (1/4, 1/8, 1/16, triplet)

### Faza 3 — Mixer & Efekty (3-4 tygodnie)

**Cel: mikser i łańcuch efektów**

- [ ] Mixer panel z faderami, panami, mute/solo
- [ ] Device Rack — łańcuch efektów
- [ ] Wbudowane efekty (EQ, Compressor, Reverb, Delay)
- [ ] Send/Return routing
- [ ] Audio graph z topological sort

### Faza 4 — Session View & Clipy (3-4 tygodnie)

**Cel: siatka klipów i improwizacja**

- [ ] Session View — grid klipów z launch/stop
- [ ] Scene launch
- [ ] Clip launch quantization
- [ ] Przełączanie Session ↔ Arrangement (`Tab`)

### Faza 5 — Plugin Hosting & Polish (4-6 tygodni)

**Cel: obsługa pluginów CLAP i dopracowanie**

- [ ] CLAP plugin hosting (clack-host)
- [ ] Plugin scanning i ładowanie
- [ ] Browser (drzewo plików, preview, drag & drop)
- [ ] Undo/Redo system
- [ ] Export mixdown (offline render → WAV/FLAC)
- [ ] Zapisywanie/wczytywanie projektów (format JSON/CBOR)

### Faza 6 — Zaawansowane (ongoing)

- [ ] Automation lanes
- [ ] VST3 hosting (vst3-sys)
- [ ] MIDI learn (mapping kontrolerów)
- [ ] Grupy ścieżek
- [ ] Time stretching / warping
- [ ] Nagrywanie MIDI z overdub
