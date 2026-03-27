# Agent-Modell Vergleich

## Haiku

**Staerken:**
- Schnell und guenstig
- Reicht fuer DOM/JS-Interaktionen
- Text-Extraktion, Button-Klicks, Navigation

**Schwaechen:**
- Braucht KONKRETE JS-Befehle im Prompt (nicht "finde den Button")
- Schlechtere Bilderkennung bei CAPTCHAs
- Verliert sich wenn Anweisungen zu vage sind

**Einsetzen fuer:**
- Cookie-Dialoge schliessen
- Suchergebnisse extrahieren
- KI-Modus Text lesen
- Einfache Formulare ausfuellen
- DOM-Annotierungen analysieren

## Sonnet

**Staerken:**
- Gute Bilderkennung (CAPTCHAs, Screenshots)
- Kann selbststaendiger navigieren
- Versteht auch vage Anweisungen

**Schwaechen:**
- Teurer als Haiku (~5x)
- Langsamer bei einfachen Aufgaben

**Einsetzen fuer:**
- CAPTCHA-Solving (Bilder erkennen)
- Komplexe Seitenanalyse wo Layout wichtig ist
- Aufgaben die visuelle Entscheidungen brauchen

## Opus

**Staerken:**
- Beste Reasoning-Faehigkeiten
- Kann komplexe mehrstufige Aufgaben planen

**Schwaechen:**
- Sehr teuer
- Overkill fuer einfache Browser-Automation

**Einsetzen fuer:**
- Komplexe Recherche-Auftraege
- Multi-Step Workflows die Planung brauchen
- Qualitaetskontrolle/Review

## Faustregel

1. **Standard: Haiku** - mit konkreten JS-Befehlen im Prompt
2. **Bilder/CAPTCHA: Sonnet** - wenn visuelle Analyse noetig
3. **Komplex: Opus** - wenn Planung und Reasoning noetig

## Debug-Tools nach Modell

| Modell | Empfohlene Debug-Tools |
|--------|----------------------|
| Haiku | `/debug/console` (Fehler finden), `/debug/cookies` (State pruefen) |
| Sonnet | Wie Haiku + `/debug/css/computed` (Layout debuggen), `/debug/network/entries` |
| Opus | Alle Debug-Tools + `/debug/performance/vitals` fuer Optimierung |

## Screenshot-Strategie nach Modell

| Modell | Navigation | CAPTCHA | Dokumentation |
|--------|-----------|---------|---------------|
| Haiku | JS/DOM (kein Screenshot) | JPEG q>=80 + Zoom | Optional |
| Sonnet | JS/DOM bevorzugt | JPEG q>=80 | Optional |
| Opus | JS/DOM bevorzugt | JPEG q>=80 | Optional |
