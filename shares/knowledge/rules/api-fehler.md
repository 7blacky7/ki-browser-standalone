# API-Fehler & Anti-Patterns

Regeln die aus Fehlern gelernt wurden. **VOR jeder Browser-Session lesen!**

---

## Screenshot-Regeln

### REGEL: Kein PNG bei Subagenten
- **Fehler:** PNG-Screenshot >200KB an Haiku/Sonnet Subagent geschickt
- **Symptom:** "Could not process image" Fehler, Agent crasht
- **Fix:** IMMER format=jpeg mit quality>=80 bei CAPTCHA-Bildern
- **Gelernt:** 2026-02-15

### REGEL: JPEG quality>=80 fuer Bilderkennung
- **Fehler:** JPEG mit quality=40 verwendet, Agent konnte Bilder nicht erkennen
- **Symptom:** Agent erkennt Objekte falsch oder gar nicht
- **Fix:** Minimum quality=80 fuer Uebersicht, 85 fuer Zoom, 90 fuer Detail
- **Gelernt:** 2026-02-15

---

## Navigation-Regeln

### REGEL: DOM vor Screenshot fuer Navigation
- **Fehler:** Haiku hat nur Screenshots analysiert statt JavaScript zu nutzen
- **Symptom:** Agent findet Buttons nicht, scrollt planlos, klickt ins Leere
- **Fix:** IMMER zuerst per JS/evaluate Elemente suchen, Screenshots NUR fuer Bildanalyse
- **Gelernt:** 2026-02-15

### REGEL: Haiku braucht konkrete JS-Befehle
- **Fehler:** Prompt sagte "finde den Akzeptieren-Button" ohne JS-Beispiel
- **Symptom:** Haiku versucht per Screenshot zu navigieren, scheitert
- **Fix:** Immer fertige curl-Befehle mit querySelector im Prompt mitgeben
- **Gelernt:** 2026-02-15

---

## Tab-Regeln

### REGEL: Subagenten duerfen KEINE Tabs erstellen
- **Fehler:** Subagent hat /tabs/new aufgerufen, Dutzende Tabs offen
- **Symptom:** Browser wird langsam, falsche Tabs werden angesprochen
- **Fix:** NUR Hauptagent erstellt Tabs, Subagent bekommt tab_id zugewiesen
- **Gelernt:** 2026-02-15

### REGEL: tab_id IMMER mitgeben
- **Fehler:** API-Call ohne tab_id, ging auf "aktiven" Tab
- **Symptom:** Falscher Tab wird angesprochen, leere/falsche Ergebnisse
- **Fix:** JEDER API-Call muss tab_id enthalten
- **Gelernt:** 2026-02-15

---

## JavaScript-Regeln

### REGEL: IMMER IIFE verwenden bei evaluate
- **Fehler:** Zwei evaluate-Calls mit `let combos=...` -> "Identifier has already been declared"
- **Symptom:** SyntaxError bei zweitem evaluate-Call im gleichen Tab
- **Fix:** ALLE JS-Snippets in IIFE wrappen: `(()=>{ let x=...; return x })()`
- **Gelernt:** 2026-02-15

### REGEL: nativeInputValueSetter fuer Google-Formulare
- **Fehler:** `/type` Endpoint setzt Wert, aber Google Material-UI Inputs erkennen es nicht
- **Symptom:** Feld sieht leer aus, Validierung schlaegt fehl ("Passwort bestaetigen")
- **Fix:** Statt /type den nativen Setter verwenden:
  ```javascript
  let nativeSet = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;
  nativeSet.call(input, 'wert');
  input.dispatchEvent(new Event('input',{bubbles:true}));
  ```
- **Gelernt:** 2026-02-15

### REGEL: Google Custom-Dropdowns sind KEINE select-Elemente
- **Fehler:** `document.querySelector("select")` gibt null zurueck bei Google-Formularen
- **Symptom:** Dropdown-Werte koennen nicht gesetzt werden
- **Fix:** Google nutzt Material Design Comboboxes: `[role=combobox]` oeffnen, dann `[role=option]` klicken
- **Gelernt:** 2026-02-15

---

## Account-Erstellung

### REGEL: Google Passwort-Bestaetigungsfeld MUSS ausgefuellt sein
- **Fehler:** Passwort-Bestaetigungsfeld (`input[name=PasswdAgain]`) war leer, Fehlerseite erschien
- **Symptom:** Fehlerseite /signup/error/1 "Ihr Google-Konto konnte nicht erstellt werden"
- **Fix:** BEIDE Passwort-Felder ausfuellen! Pattern: `/click` mit selector -> `/type` OHNE selector.
  VOR dem Klick auf "Weiter" per JS pruefen: `input[name=Passwd].value === input[name=PasswdAgain].value`
- **WARNUNG:** Wurde faelschlicherweise als "Bot-Erkennung" diagnostiziert - war nur leeres Feld!
- **Gelernt:** 2026-02-15

### REGEL: Google Passwort-Felder mit click-focus + type ohne selector
- **Fehler:** `/type` mit selector auf Google Passwort-Feld -> Wert wird nicht erkannt
- **Symptom:** Feld sieht leer aus oder Wert wird nicht von Material-UI akzeptiert
- **Fix:** 1) `/click` mit selector aufs Feld (fokussiert es), 2) `/type` OHNE selector (tippt in fokussiertes Feld)
- **Gelernt:** 2026-02-15

### REGEL: Google Signup - Button-Indices beachten bei Express-Einstellungen
- **Fehler:** Falschen Button geklickt weil "Weitere Informationen"-Links wie Hauptbuttons aussehen
- **Symptom:** Info-Dialog oeffnet sich statt Einstellungen zu bestaetigen
- **Fix:** Button-Indices: [0-2]=Info-Links, [3]=Alle akzeptieren, [4]=Alle ablehnen, [5]=Weitere Optionen
- **Gelernt:** 2026-02-15

---

## Google-Konto Regeln

### REGEL: Bot-erstellte Google-Konten werden innerhalb von Stunden deaktiviert
- **Fehler:** Google-Konto per ki-browser erstellt, QR-Verifizierung mit Smartphone durchgefuehrt
- **Symptom:** Beim naechsten Login: "Ihr Konto wurde deaktiviert" - "Es wurde moeglicherweise von einem Computerprogramm oder Bot erstellt"
- **Fix:** Google erkennt Bot-Erstellung zuverlaessig. KEINE Google-Konten per Automation erstellen. Stattdessen bestehende Konten verwenden oder manuelle Erstellung.
- **WARNUNG:** Die QR-Code-Verifizierung verknuepft die Telefonnummer DOCH mit dem Konto (trotz gegenteiliger Anzeige). Google nutzt sie spaeter fuer SMS-Verifizierung.
- **Gelernt:** 2026-02-15

### REGEL: Google Closure-Buttons reagieren NICHT auf JS-Clicks
- **Fehler:** Google "Weiter"/"Senden" Buttons mit jscontroller/jsaction per JavaScript geklickt
- **Symptom:** `.click()`, `dispatchEvent(MouseEvent)`, Enter-Taste - alles ignoriert
- **Fix:** CDP-Koordinaten-Klicks via `/click` API mit EXAKTEN Pixel-Koordinaten. Position per `getBoundingClientRect()` ermitteln. Manchmal mehrere Versuche noetig.
- **Pattern:** 1) `evaluate` -> `el.getBoundingClientRect()` -> x,y merken, 2) `/click` mit `x,y` (OHNE selector)
- **Gelernt:** 2026-02-15

---

## Formular-Framework Regeln

### REGEL: nativeInputValueSetter funktioniert NICHT bei Vue/React-Formularen (Gameforge etc.)
- **Fehler:** nativeInputValueSetter bei OGame/Gameforge Registrierung verwendet, Passwort wurde nicht an Server uebermittelt
- **Symptom:** Login nach Registrierung schlaegt fehl: "Unbekannte E-Mail-Adresse oder falsches Passwort"
- **Fix:** IMMER click-focus + `/type` OHNE selector verwenden. nativeInputValueSetter setzt nur DOM-Value, aber Vue/React-State bleibt leer.
- **Unterschied:** Bei Google (Material-UI) funktioniert nativeInputValueSetter, bei Gameforge (Vue) NICHT.
- **Gelernt:** 2026-02-15

### REGEL: Sonderzeichen (!) in Passwoertern - JSON escaping beachten
- **Fehler:** `curl -d '{"text":"Passwort2026!Secure"}'` verursacht "invalid escape" JSON-Fehler
- **Symptom:** Bash interpretiert `!` in single-quotes je nach Kontext, JSON wird ungueltig
- **Fix:** Python fuer JSON-Payload verwenden: `python3 -c "import json; print(json.dumps({'text':'Passwort2026!Secure'}))"` oder Heredoc verwenden
- **Gelernt:** 2026-02-15

---

## CAPTCHA-Regeln

### REGEL: reCAPTCHA Bild-Challenge mit Sonnet loesen
- **Fehler:** Manuell zu langsam bei Bildauswahl -> CAPTCHA expired
- **Symptom:** "Bestaetigungsaufforderung abgelaufen" nach zu langer Wartezeit
- **Fix:** Sonnet-Subagent fuer Bildanalyse (JPEG quality>=80), schnell Zellen auswaehlen + sofort BESTAETIGEN klicken. NICHT zu lange warten.
- **Pattern:** Screenshot als JPEG -> Sonnet analysiert Grid -> Zellen klicken -> BESTAETIGEN
- **Gelernt:** 2026-02-15

---

## OAuth-Regeln

### REGEL: Gameforge OAuth-URL muss per fetch-Interception abgefangen werden
- **Fehler:** "Mit Google einloggen" oeffnet Popup das im Headless blockiert wird
- **Symptom:** window.open wird aufgerufen, aber kein Popup erscheint
- **Fix:** `window.fetch` ueberschreiben, `auth/external` Response abfangen, URL direkt im Tab oeffnen
- **Pattern:**
  ```javascript
  var origFetch = window.fetch;
  window.fetch = function(url, opts) {
    if(url.includes('auth/external')) {
      return origFetch.apply(this, arguments).then(r => {
        r.clone().json().then(d => { window._authResponse = d; });
        return r;
      });
    }
    return origFetch.apply(this, arguments);
  };
  ```
- **Gelernt:** 2026-02-15

---

## OGame-spezifische Regeln

### REGEL: OGame "Weiter"-Button per form.submit() statt click
- **Fehler:** Normaler Klick auf den "Weiter"-Button auf der Intro-Seite hat nicht funktioniert
- **Symptom:** Button wird geklickt, aber Formular wird nicht abgeschickt
- **Fix:** `form.submit()` per JavaScript evaluate verwenden statt normalen Klick
- **Gelernt:** 2026-02-15

### REGEL: OGame Spiel-Launch per fetch-Interception
- **Fehler:** "Spielen"-Button oeffnet window.open("/loading") - blockiert im Headless
- **Symptom:** Kein neues Fenster, Spiel startet nicht
- **Fix:** Fetch-Interceptor fuer `/api/users/me/loginLink` setzen + window.open blockieren.
  Button klicken, Game-URL aus Interceptor auslesen, per /navigate dorthin.
  WICHTIG: Der `blackbox` Anti-Bot-Token kann NUR vom React-Client generiert werden!
- **Pattern:** Siehe Site-Profil lobby-ogame-gameforge-com.md, Abschnitt "Spiel-Launch-Flow"
- **Gelernt:** 2026-02-15

### REGEL: OGame Gebaeude - Nur 1 gleichzeitig, Countdown abwarten
- **Fehler:** Zweites Gebaeude gestartet waehrend erstes noch im Bau
- **Symptom:** Ausbauen-Button nicht vorhanden oder disabled
- **Fix:** IMMER `.buildCountdown` / `.countdown` Element pruefen und warten bis leer/verschwunden
- **Gelernt:** 2026-02-15

### REGEL: OGame Technology-Elemente haben Duplikate im DOM
- **Fehler:** `querySelectorAll('[data-technology="1"]')` gibt 2 Elemente zurueck
- **Symptom:** Falsches Element geklickt, Level-Auslesen gibt 0 zurueck
- **Fix:** IMMER `els[0]` (erstes Element) verwenden - zweites ist nur ein Icon-Sprite
- **Gelernt:** 2026-02-15

### REGEL: OGame Happy Hour Popup blockiert Interaktion
- **Fehler:** Happy Hour Popup erscheint nach Seitenwechsel, verdeckt alle Buttons
- **Symptom:** Klicks gehen ins Leere, Elemente nicht erreichbar
- **Fix:** Vor jeder Interaktion Popups schliessen: `button.ui-button.ui-dialog-titlebar-close` klicken
- **Gelernt:** 2026-02-15

### REGEL: OGame Technology-Cards haben KEINE Upgrade-Buttons
- **Fehler:** `button.upgrade` auf `[data-technology]` Elementen gesucht
- **Symptom:** Button ist immer `null`, obwohl Ressourcen ausreichen
- **Fix:** Technology-Cards (`LI.technology`) enthalten NUR Sprite + Level-Anzeige.
  Der Upgrade-Button existiert nur im Details-Panel das per AJAX geladen wird.
  BESSER: Direkt per `buildListActionBuild(techId, 1, 1, null, null)` bauen!
- **Gelernt:** 2026-02-15

### REGEL: OGame Build-Token rotiert nach jedem Bau
- **Fehler:** Gleichen `token` fuer mehrere aufeinanderfolgende Builds verwendet
- **Symptom:** Server antwortet "An error has occured!" (HTTP 200, aber HTML statt JSON)
- **Fix:** Jeder Build-Request gibt `newAjaxToken` in der JSON-Response zurueck.
  Diesen Token MUSS fuer den naechsten Build verwendet werden.
  `$.ajax` Override um Response abzufangen, oder Seite neu laden fuer frischen Token.
- **Pattern:**
  ```javascript
  // Token-Rotation: Override $.ajax, capture newAjaxToken
  buildListActionCalled = false;
  const origAjax = $.ajax;
  $.ajax = function(opts) {
    opts.success = function(data) { if(data.newAjaxToken) token = data.newAjaxToken; };
    return origAjax.call(this, opts);
  };
  buildListActionBuild(techId, 1, 1, null, null);
  ```
- **Gelernt:** 2026-02-15

### REGEL: OGame Bau per AJAX statt UI-Klick
- **Fehler:** Versucht den Upgrade-Button per CSS-Selector/Koordinaten zu klicken
- **Symptom:** Button nicht gefunden, oder Klick hat keine Wirkung
- **Fix:** OGame hat globale JS-Funktionen fuer den Bau:
  - `buildListActionBuild(technologyId, amount, mode, buyWithDm, planetId)` - Bau starten
  - `executeBuildAction(technologyId, planetId, mode, listId)` - Alternative
  - `scheduleBuildListEntryUrl` - AJAX Endpoint URL
  - `token` - CSRF Token (global, rotiert nach jedem Build)
  POST an `scheduleBuildListEntryUrl` mit `{technologyId, amount:1, mode:1, token}`
- **Gelernt:** 2026-02-15

---

## Viewport-Regeln

### REGEL: Full HD (1920x1080) als Standard-Viewport verwenden
- **Fehler:** Browser mit 1280x720 gestartet, Consent-Dialoge waren zusammengequetscht und Buttons schwer zu finden
- **Symptom:** Overlays abgeschnitten, Buttons ausserhalb des sichtbaren Bereichs, Klicks gehen ins Leere
- **Fix:** IMMER `--width 1920 --height 1080` verwenden. Bei Consent-Dialogen hat man so genug Platz und sieht die Seite dahinter.
- **Gelernt:** 2026-03-27

### REGEL: Consent-iFrame-Buttons per DOM-Koordinaten klicken, nicht per Screenshot-Position
- **Fehler:** Visueller Button-Position im Screenshot vertraut, aber Klick landete auf dem Overlay statt im iFrame
- **Symptom:** Klicks bei (395,184) gehen ins Leere, obwohl dort der Button sichtbar ist
- **Fix:** Bei Consent-Dialogen (Sourcepoint, Cookiebot etc.) den iFrame per `document.getElementById('sp_message_iframe_*').getBoundingClientRect()` lokalisieren und auf dessen **Mitte** klicken. Beispiel: `{"x": Math.round(rect.x + rect.width/2), "y": Math.round(rect.y + rect.height/2)}`. Screenshot-Positionen stimmen NICHT mit klickbaren DOM-Positionen ueberein wenn iFrames im Spiel sind.
- **Pattern:**
  ```javascript
  // 1. iFrame finden
  var f = document.querySelector('iframe[id*="sp_message"]');
  var r = f.getBoundingClientRect();
  // 2. Mitte berechnen
  var x = Math.round(r.x + r.width/2);
  var y = Math.round(r.y + r.height/2);
  // 3. Per /click API klicken
  ```
- **Gelernt:** 2026-03-27

---

## Neue Regel hinzufuegen

Format:
```markdown
### REGEL: Kurze Beschreibung
- **Fehler:** Was ist passiert
- **Symptom:** Woran erkennt man den Fehler
- **Fix:** Wie vermeidet man ihn
- **Gelernt:** YYYY-MM-DD
```
