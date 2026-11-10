/**
 * Client-safe join between the ISO-3166 alpha-2 codes Umami and Cloudflare report and the feature ids of
 * the world map's geo data (`visionscarto-world-atlas` 110m: ISO-3166 numeric ids as zero-padded strings,
 * plus negative ids for three shapes with no numeric code).
 *
 * Generated once from the ISO-3166 table (alpha-2 ↔ numeric) joined with the map's own features, so it
 * lists exactly the countries the map can draw; `country-geo.test.ts` pins it against the atlas file. A
 * code missing here (Singapore, Hong Kong, Malta, and other places too small for 110m) isn't on the map,
 * and the section lists those under it. Antarctica is left out on purpose: the map doesn't draw it.
 */

/** Alpha-2 → the map feature ids it fills. Two codes fill two shapes each, because geolocation reports them there. */
const mapIdsByIso2: Readonly<Record<string, readonly string[]>> = {
  AE: ['784'], // United Arab Emirates
  AF: ['004'], // Afghanistan
  AL: ['008'], // Albania
  AM: ['051'], // Armenia
  AO: ['024'], // Angola
  AR: ['032'], // Argentina
  AT: ['040'], // Austria
  AU: ['036'], // Australia
  AZ: ['031'], // Azerbaijan
  BA: ['070'], // Bosnia and Herz.
  BD: ['050'], // Bangladesh
  BE: ['056'], // Belgium
  BF: ['854'], // Burkina Faso
  BG: ['100'], // Bulgaria
  BI: ['108'], // Burundi
  BJ: ['204'], // Benin
  BN: ['096'], // Brunei
  BO: ['068'], // Bolivia
  BR: ['076'], // Brazil
  BS: ['044'], // Bahamas
  BT: ['064'], // Bhutan
  BW: ['072'], // Botswana
  BY: ['112'], // Belarus
  BZ: ['084'], // Belize
  CA: ['124'], // Canada
  CD: ['180'], // Dem. Rep. Congo
  CF: ['140'], // Central African Rep.
  CG: ['178'], // Congo
  CH: ['756'], // Switzerland
  CI: ['384'], // Côte d'Ivoire
  CL: ['152'], // Chile
  CM: ['120'], // Cameroon
  CN: ['156'], // China
  CO: ['170'], // Colombia
  CR: ['188'], // Costa Rica
  CU: ['192'], // Cuba
  CY: ['196', '-1'], // Cyprus, plus N. Cyprus (geolocation reports CY there)
  CZ: ['203'], // Czechia
  DE: ['276'], // Germany
  DJ: ['262'], // Djibouti
  DK: ['208'], // Denmark
  DO: ['214'], // Dominican Rep.
  DZ: ['012'], // Algeria
  EC: ['218'], // Ecuador
  EE: ['233'], // Estonia
  EG: ['818'], // Egypt
  EH: ['732'], // W. Sahara
  ER: ['232'], // Eritrea
  ES: ['724'], // Spain
  ET: ['231'], // Ethiopia
  FI: ['246'], // Finland
  FJ: ['242'], // Fiji
  FK: ['238'], // Falkland Is.
  FR: ['250'], // France
  GA: ['266'], // Gabon
  GB: ['826'], // United Kingdom
  GE: ['268'], // Georgia
  GH: ['288'], // Ghana
  GL: ['304'], // Greenland
  GM: ['270'], // Gambia
  GN: ['324'], // Guinea
  GQ: ['226'], // Eq. Guinea
  GR: ['300'], // Greece
  GT: ['320'], // Guatemala
  GW: ['624'], // Guinea-Bissau
  GY: ['328'], // Guyana
  HN: ['340'], // Honduras
  HR: ['191'], // Croatia
  HT: ['332'], // Haiti
  HU: ['348'], // Hungary
  ID: ['360'], // Indonesia
  IE: ['372'], // Ireland
  IL: ['376'], // Israel
  IN: ['356'], // India
  IQ: ['368'], // Iraq
  IR: ['364'], // Iran
  IS: ['352'], // Iceland
  IT: ['380'], // Italy
  JM: ['388'], // Jamaica
  JO: ['400'], // Jordan
  JP: ['392'], // Japan
  KE: ['404'], // Kenya
  KG: ['417'], // Kyrgyzstan
  KH: ['116'], // Cambodia
  KP: ['408'], // North Korea
  KR: ['410'], // South Korea
  KW: ['414'], // Kuwait
  KZ: ['398'], // Kazakhstan
  LA: ['418'], // Laos
  LB: ['422'], // Lebanon
  LK: ['144'], // Sri Lanka
  LR: ['430'], // Liberia
  LS: ['426'], // Lesotho
  LT: ['440'], // Lithuania
  LU: ['442'], // Luxembourg
  LV: ['428'], // Latvia
  LY: ['434'], // Libya
  MA: ['504'], // Morocco
  MD: ['498'], // Moldova
  ME: ['499'], // Montenegro
  MG: ['450'], // Madagascar
  MK: ['807'], // North Macedonia
  ML: ['466'], // Mali
  MM: ['104'], // Myanmar
  MN: ['496'], // Mongolia
  MR: ['478'], // Mauritania
  MW: ['454'], // Malawi
  MX: ['484'], // Mexico
  MY: ['458'], // Malaysia
  MZ: ['508'], // Mozambique
  NA: ['516'], // Namibia
  NC: ['540'], // New Caledonia
  NE: ['562'], // Niger
  NG: ['566'], // Nigeria
  NI: ['558'], // Nicaragua
  NL: ['528'], // Netherlands
  NO: ['578'], // Norway
  NP: ['524'], // Nepal
  NZ: ['554'], // New Zealand
  OM: ['512'], // Oman
  PA: ['591'], // Panama
  PE: ['604'], // Peru
  PG: ['598'], // Papua New Guinea
  PH: ['608'], // Philippines
  PK: ['586'], // Pakistan
  PL: ['616'], // Poland
  PR: ['630'], // Puerto Rico
  PS: ['275'], // Palestine
  PT: ['620'], // Portugal
  PY: ['600'], // Paraguay
  QA: ['634'], // Qatar
  RO: ['642'], // Romania
  RS: ['688'], // Serbia
  RU: ['643'], // Russia
  RW: ['646'], // Rwanda
  SA: ['682'], // Saudi Arabia
  SB: ['090'], // Solomon Is.
  SD: ['729'], // Sudan
  SE: ['752'], // Sweden
  SI: ['705'], // Slovenia
  SK: ['703'], // Slovakia
  SL: ['694'], // Sierra Leone
  SN: ['686'], // Senegal
  SO: ['706', '-3'], // Somalia, plus Somaliland (geolocation reports SO there)
  SR: ['740'], // Suriname
  SS: ['728'], // S. Sudan
  SV: ['222'], // El Salvador
  SY: ['760'], // Syria
  SZ: ['748'], // eSwatini
  TD: ['148'], // Chad
  TF: ['260'], // Fr. S. Antarctic Lands
  TG: ['768'], // Togo
  TH: ['764'], // Thailand
  TJ: ['762'], // Tajikistan
  TL: ['626'], // Timor-Leste
  TM: ['795'], // Turkmenistan
  TN: ['788'], // Tunisia
  TR: ['792'], // Turkey
  TT: ['780'], // Trinidad and Tobago
  TW: ['158'], // Taiwan
  TZ: ['834'], // Tanzania
  UA: ['804'], // Ukraine
  UG: ['800'], // Uganda
  US: ['840'], // United States of America
  UY: ['858'], // Uruguay
  UZ: ['860'], // Uzbekistan
  VE: ['862'], // Venezuela
  VN: ['704'], // Vietnam
  VU: ['548'], // Vanuatu
  XK: ['-2'], // Kosovo (user-assigned XK, which Cloudflare and MaxMind use)
  YE: ['887'], // Yemen
  ZA: ['710'], // South Africa
  ZM: ['894'], // Zambia
  ZW: ['716'], // Zimbabwe
}

const iso2ByMapId = new Map(Object.entries(mapIdsByIso2).flatMap(([iso2, ids]) => ids.map((id) => [id, iso2])))

/** The map feature ids for an alpha-2 code (any case), or `[]` when the map doesn't have that country. */
export function mapIdsForCountry(iso2: string): readonly string[] {
  return mapIdsByIso2[iso2.toUpperCase()] ?? []
}

/** The alpha-2 code a map feature stands for, or `null` for a shape we don't map (Antarctica). */
export function countryForMapId(id: string): string | null {
  return iso2ByMapId.get(id) ?? null
}

/** The map feature id the map leaves out: Antarctica, which only stretches the projection. */
export const antarcticaMapId = '010'
