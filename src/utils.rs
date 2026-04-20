use serde_json::Value;

use crate::{
    client::ReqwestClient,
    errors::{AppError, Result},
    models::{
        age::{AgeGroup, AgifyResponse},
        country::{NationalizeRawResponse, NationalizeResponse},
        gender::GenderizeResponse,
    },
};

// Nationalize API returns only a country's ID and not it's name.
/// This function resolves an ISO 3166-1 alpha-2 code to a full country name.
/// The slice is sorted by code so we can binary-search in O(log n).
// Binary search here instead of a static HashMap is more cost-effective at runtime
// for this task.
fn iso_to_country_name(code: &str) -> &'static str {
    const TABLE: &[(&str, &str)] = &[
        ("AD", "Andorra"),
        ("AE", "United Arab Emirates"),
        ("AF", "Afghanistan"),
        ("AG", "Antigua and Barbuda"),
        ("AI", "Anguilla"),
        ("AL", "Albania"),
        ("AM", "Armenia"),
        ("AO", "Angola"),
        ("AQ", "Antarctica"),
        ("AR", "Argentina"),
        ("AS", "American Samoa"),
        ("AT", "Austria"),
        ("AU", "Australia"),
        ("AW", "Aruba"),
        ("AX", "Aland Islands"),
        ("AZ", "Azerbaijan"),
        ("BA", "Bosnia and Herzegovina"),
        ("BB", "Barbados"),
        ("BD", "Bangladesh"),
        ("BE", "Belgium"),
        ("BF", "Burkina Faso"),
        ("BG", "Bulgaria"),
        ("BH", "Bahrain"),
        ("BI", "Burundi"),
        ("BJ", "Benin"),
        ("BL", "Saint Barthelemy"),
        ("BM", "Bermuda"),
        ("BN", "Brunei"),
        ("BO", "Bolivia"),
        ("BQ", "Caribbean Netherlands"),
        ("BR", "Brazil"),
        ("BS", "Bahamas"),
        ("BT", "Bhutan"),
        ("BV", "Bouvet Island"),
        ("BW", "Botswana"),
        ("BY", "Belarus"),
        ("BZ", "Belize"),
        ("CA", "Canada"),
        ("CC", "Cocos Islands"),
        ("CD", "Democratic Republic of the Congo"),
        ("CF", "Central African Republic"),
        ("CG", "Republic of the Congo"),
        ("CH", "Switzerland"),
        ("CI", "Ivory Coast"),
        ("CK", "Cook Islands"),
        ("CL", "Chile"),
        ("CM", "Cameroon"),
        ("CN", "China"),
        ("CO", "Colombia"),
        ("CR", "Costa Rica"),
        ("CU", "Cuba"),
        ("CV", "Cape Verde"),
        ("CW", "Curacao"),
        ("CX", "Christmas Island"),
        ("CY", "Cyprus"),
        ("CZ", "Czech Republic"),
        ("DE", "Germany"),
        ("DJ", "Djibouti"),
        ("DK", "Denmark"),
        ("DM", "Dominica"),
        ("DO", "Dominican Republic"),
        ("DZ", "Algeria"),
        ("EC", "Ecuador"),
        ("EE", "Estonia"),
        ("EG", "Egypt"),
        ("EH", "Western Sahara"),
        ("ER", "Eritrea"),
        ("ES", "Spain"),
        ("ET", "Ethiopia"),
        ("FI", "Finland"),
        ("FJ", "Fiji"),
        ("FK", "Falkland Islands"),
        ("FM", "Micronesia"),
        ("FO", "Faroe Islands"),
        ("FR", "France"),
        ("GA", "Gabon"),
        ("GB", "United Kingdom"),
        ("GD", "Grenada"),
        ("GE", "Georgia"),
        ("GF", "French Guiana"),
        ("GG", "Guernsey"),
        ("GH", "Ghana"),
        ("GI", "Gibraltar"),
        ("GL", "Greenland"),
        ("GM", "Gambia"),
        ("GN", "Guinea"),
        ("GP", "Guadeloupe"),
        ("GQ", "Equatorial Guinea"),
        ("GR", "Greece"),
        ("GS", "South Georgia and the South Sandwich Islands"),
        ("GT", "Guatemala"),
        ("GU", "Guam"),
        ("GW", "Guinea-Bissau"),
        ("GY", "Guyana"),
        ("HK", "Hong Kong"),
        ("HM", "Heard Island and McDonald Islands"),
        ("HN", "Honduras"),
        ("HR", "Croatia"),
        ("HT", "Haiti"),
        ("HU", "Hungary"),
        ("ID", "Indonesia"),
        ("IE", "Ireland"),
        ("IL", "Israel"),
        ("IM", "Isle of Man"),
        ("IN", "India"),
        ("IO", "British Indian Ocean Territory"),
        ("IQ", "Iraq"),
        ("IR", "Iran"),
        ("IS", "Iceland"),
        ("IT", "Italy"),
        ("JE", "Jersey"),
        ("JM", "Jamaica"),
        ("JO", "Jordan"),
        ("JP", "Japan"),
        ("KE", "Kenya"),
        ("KG", "Kyrgyzstan"),
        ("KH", "Cambodia"),
        ("KI", "Kiribati"),
        ("KM", "Comoros"),
        ("KN", "Saint Kitts and Nevis"),
        ("KP", "North Korea"),
        ("KR", "South Korea"),
        ("KW", "Kuwait"),
        ("KY", "Cayman Islands"),
        ("KZ", "Kazakhstan"),
        ("LA", "Laos"),
        ("LB", "Lebanon"),
        ("LC", "Saint Lucia"),
        ("LI", "Liechtenstein"),
        ("LK", "Sri Lanka"),
        ("LR", "Liberia"),
        ("LS", "Lesotho"),
        ("LT", "Lithuania"),
        ("LU", "Luxembourg"),
        ("LV", "Latvia"),
        ("LY", "Libya"),
        ("MA", "Morocco"),
        ("MC", "Monaco"),
        ("MD", "Moldova"),
        ("ME", "Montenegro"),
        ("MF", "Saint Martin"),
        ("MG", "Madagascar"),
        ("MH", "Marshall Islands"),
        ("MK", "North Macedonia"),
        ("ML", "Mali"),
        ("MM", "Myanmar"),
        ("MN", "Mongolia"),
        ("MO", "Macao"),
        ("MP", "Northern Mariana Islands"),
        ("MQ", "Martinique"),
        ("MR", "Mauritania"),
        ("MS", "Montserrat"),
        ("MT", "Malta"),
        ("MU", "Mauritius"),
        ("MV", "Maldives"),
        ("MW", "Malawi"),
        ("MX", "Mexico"),
        ("MY", "Malaysia"),
        ("MZ", "Mozambique"),
        ("NA", "Namibia"),
        ("NC", "New Caledonia"),
        ("NE", "Niger"),
        ("NF", "Norfolk Island"),
        ("NG", "Nigeria"),
        ("NI", "Nicaragua"),
        ("NL", "Netherlands"),
        ("NO", "Norway"),
        ("NP", "Nepal"),
        ("NR", "Nauru"),
        ("NU", "Niue"),
        ("NZ", "New Zealand"),
        ("OM", "Oman"),
        ("PA", "Panama"),
        ("PE", "Peru"),
        ("PF", "French Polynesia"),
        ("PG", "Papua New Guinea"),
        ("PH", "Philippines"),
        ("PK", "Pakistan"),
        ("PL", "Poland"),
        ("PM", "Saint Pierre and Miquelon"),
        ("PN", "Pitcairn"),
        ("PR", "Puerto Rico"),
        ("PS", "Palestine"),
        ("PT", "Portugal"),
        ("PW", "Palau"),
        ("PY", "Paraguay"),
        ("QA", "Qatar"),
        ("RE", "Reunion"),
        ("RO", "Romania"),
        ("RS", "Serbia"),
        ("RU", "Russia"),
        ("RW", "Rwanda"),
        ("SA", "Saudi Arabia"),
        ("SB", "Solomon Islands"),
        ("SC", "Seychelles"),
        ("SD", "Sudan"),
        ("SE", "Sweden"),
        ("SG", "Singapore"),
        ("SH", "Saint Helena"),
        ("SI", "Slovenia"),
        ("SJ", "Svalbard and Jan Mayen"),
        ("SK", "Slovakia"),
        ("SL", "Sierra Leone"),
        ("SM", "San Marino"),
        ("SN", "Senegal"),
        ("SO", "Somalia"),
        ("SR", "Suriname"),
        ("SS", "South Sudan"),
        ("ST", "Sao Tome and Principe"),
        ("SV", "El Salvador"),
        ("SX", "Sint Maarten"),
        ("SY", "Syria"),
        ("SZ", "Eswatini"),
        ("TC", "Turks and Caicos Islands"),
        ("TD", "Chad"),
        ("TF", "French Southern Territories"),
        ("TG", "Togo"),
        ("TH", "Thailand"),
        ("TJ", "Tajikistan"),
        ("TK", "Tokelau"),
        ("TL", "Timor-Leste"),
        ("TM", "Turkmenistan"),
        ("TN", "Tunisia"),
        ("TO", "Tonga"),
        ("TR", "Turkey"),
        ("TT", "Trinidad and Tobago"),
        ("TV", "Tuvalu"),
        ("TW", "Taiwan"),
        ("TZ", "Tanzania"),
        ("UA", "Ukraine"),
        ("UG", "Uganda"),
        ("UM", "United States Minor Outlying Islands"),
        ("US", "United States"),
        ("UY", "Uruguay"),
        ("UZ", "Uzbekistan"),
        ("VA", "Vatican City"),
        ("VC", "Saint Vincent and the Grenadines"),
        ("VE", "Venezuela"),
        ("VG", "British Virgin Islands"),
        ("VI", "United States Virgin Islands"),
        ("VN", "Vietnam"),
        ("VU", "Vanuatu"),
        ("WF", "Wallis and Futuna"),
        ("WS", "Samoa"),
        ("YE", "Yemen"),
        ("YT", "Mayotte"),
        ("ZA", "South Africa"),
        ("ZM", "Zambia"),
        ("ZW", "Zimbabwe"),
    ];

    let uppercase_code = code.to_uppercase();

    TABLE
        .binary_search_by_key(&uppercase_code.as_str(), |&(key, _)| key)
        .map(|idx| TABLE[idx].1)
        .unwrap_or("Unknown")
}

pub fn validate_name(name_value: Option<Value>) -> Result<String> {
    match name_value {
        None => Err(AppError::BadRequest("Missing or empty name".to_string())),
        Some(Value::String(name)) => {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                Err(AppError::BadRequest("Missing or empty name".to_string()))
            } else {
                Ok(trimmed)
            }
        }
        Some(_) => Err(AppError::UnprocessableEntity("Invalid type".to_string())),
    }
}

pub async fn fetch_gender_data(
    reqwest_client: &ReqwestClient,
    name: &str,
) -> Result<GenderizeResponse> {
    let client = reqwest_client.get();
    let response: GenderizeResponse = client
        .get("https://api.genderize.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    if response.gender.is_none() || response.sample_size == 0 {
        return Err(AppError::UpstreamInvalidResponse("Genderize".to_string()));
    }

    Ok(response)
}

pub async fn fetch_age_data(reqwest_client: &ReqwestClient, name: &str) -> Result<AgifyResponse> {
    let client = reqwest_client.get();
    let mut response: AgifyResponse = client
        .get("https://api.agify.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    if response.age.is_none() {
        return Err(AppError::UpstreamInvalidResponse("Agify".to_string()));
    }

    response.age_group = AgeGroup::classify(response.age.unwrap_or(0));

    Ok(response)
}

pub async fn fetch_country_data(
    reqwest_client: &ReqwestClient,
    name: &str,
) -> Result<NationalizeResponse> {
    let client = reqwest_client.get();
    let response: NationalizeRawResponse = client
        .get("https://api.nationalize.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    let best_country = response
        .country
        .into_iter()
        .max_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| AppError::UpstreamInvalidResponse("Nationalize".to_string()))?;

    Ok(NationalizeResponse {
        country_name: iso_to_country_name(&best_country.country_id).to_string(),
        country_id: best_country.country_id,
        country_probability: best_country.probability,
    })
}
