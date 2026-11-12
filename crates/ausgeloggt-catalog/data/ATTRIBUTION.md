# Attribution of bundled data

Snapshot taken 2026-11-12. Both files are trimmed to the fields ausgeloggt uses.

## data/jdm-sites.json: JustDeleteMe

- Source: https://github.com/jdm-contrib/jdm (`_data/sites.json`, branch `master`)
- License: MIT. Copyright (c) 2013-2020 Robb Lewis, The JDM Contrib Team, & various contributors
- Kept fields: name, domains, url, difficulty, notes, email. 2660 entries.

The full MIT license text of the upstream project:

The MIT License (MIT)

Copyright (c) 2013-2020 Robb Lewis, The JDM Contrib Team, & various contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.



## data/companies.json: Datenanfragen.de company database

- Source: https://github.com/datenanfragen/data (`companies/*.json`, commit b48376208495b299335c775ecdb15d430d3ff905)
- License: CC0 1.0 Universal (public domain dedication); attribution is not required but given here as a courtesy.
- Kept fields: name, web host, contact e-mail, postal address, relevant countries, slug. 2785 entries.


## data/2fa.json: 2fa.directory

- Source: https://api.2fa.directory/v4/all.json (data of https://github.com/2factorauth/twofactorauth, commit 9b39477ccf259f022f694b4f14ed79c529b1039d), snapshot 2026-11-12
- License: MIT. Copyright (c) 2021 2factorauth and Contributors (2014-2020 portions: The MIT License, same terms; see https://github.com/2factorauth/twofactorauth/blob/master/LICENSE.md). Redistribution allowed provided the copyright notice and permission notice are included (this entry).
- Kept fields: domain (additional domains are already expanded as own keys upstream), methods, documentation. Only entries with at least one 2FA method: 2453 entries (~280 KB).

## src/orgs.rs: hand-curated organizations and authorities

Facts (addresses, URLs) are not copyrightable; compiled by hand, snapshot 2026-11-12.

- Data-protection authorities: addresses, e-mails and complaint-form links taken from the BfDI directory https://www.bfdi.bund.de/DE/Service/Anschriften/Laender/Laender-node.html (detail pages under /SharedDocs/Adressen/DE/LfD/). BfDI itself: Graurheindorfer Str. 153, 53117 Bonn (from knowledge; complaint form https://www.bfdi.bund.de/SharedDocs/Downloads/DE/DokumenteBfDI/Formular-Beschwerde.html). Bavaria: BayLDA (private sector) https://www.lda.bayern.de/de/beschwerde.html, Ansbach address from knowledge.
- Credit bureaus / address brokers: addresses and e-mails cross-checked with the Datenanfragen.de database (CC0, `companies.json`, slugs schufa, crifbuergel, infoscore, experian-de, deutsche-post-direkt, az-direct, acxiom, schober), plus SCHUFA Verbraucherservice PO box (Postfach 10 34 41, 50474 Köln, from knowledge), Boniversum https://www.boniversum.de/selbstauskunft/, SCHUFA data copy https://www.meineschufa.de/de/datenkopie, Robinsonliste https://www.robinsonliste.de/ (DDV, Hahnstraße 70, 60528 Frankfurt, from knowledge).
- Search-removal, payment-hub and alias-service links: official vendor pages; HTTP reachability checked 2026-11-12 (PayPal answers 403 to bots only).
