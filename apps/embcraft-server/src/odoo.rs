//! The Odoo connector (Odoo 19 and later, through its JSON-2 API with an API key).
//!
//! - machines: work centers (`mrp.workcenter`);
//! - what names a design: operations (`mrp.routing.workcenter`) on those work centers;
//! - work: work orders (`mrp.workorder`) on those work centers. A work order started in Odoo
//!   (in the Shop Floor app, or from the MO) has the state `progress`; the library then loads
//!   its design. The quantity and deadline come from the MO, the customer from the sale order
//!   the MO came from;
//! - notes: on the MO's chatter.
//!
//! It only reads, apart from the notes (an option of the connection).

use crate::connections::{Connector, Publish, Published, Remote, RemoteCustomer, Work, WorkState};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Odoo {
    base: String,
    database: String,
    key: String,
    agent: ureq::Agent,
    /// The partner field holding the old customer number, if set in the connection's settings.
    old_number_field: Option<String>,
}

/// The old-number field found on each Odoo (address and database), for an hour: finding it
/// reads every field of res.partner.
static OLD_FIELDS: std::sync::Mutex<Vec<(String, Option<String>, std::time::Instant)>> =
    std::sync::Mutex::new(Vec::new());

impl Odoo {
    /// `settings`: `{"url": "https://odoo.example.com", "database": "optional"}`.
    pub fn new(settings: &Value, key: &str) -> Result<Self, String> {
        let url = settings
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if url.is_empty() {
            return Err("Give the Odoo address (e.g. https://odoo.example.com).".into());
        }
        if key.is_empty() {
            return Err("Give an Odoo API key (Odoo › My Preferences › Account Security).".into());
        }
        let base = if url.starts_with("http://") || url.starts_with("https://") {
            url.trim_end_matches('/').to_string()
        } else {
            format!("https://{}", url.trim_end_matches('/'))
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            base,
            database: settings
                .get("database")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            key: key.to_string(),
            agent,
            old_number_field: settings
                .get("old_number_field")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(String::from),
        })
    }

    /// One call: `POST /json/2/<model>/<method>` with the named arguments.
    fn call(&self, model: &str, method: &str, args: Value) -> Result<Value, String> {
        let url = format!("{}/json/2/{model}/{method}", self.base);
        let mut req = self
            .agent
            .post(&url)
            .header("Authorization", &format!("bearer {}", self.key))
            .header("Content-Type", "application/json");
        if !self.database.is_empty() {
            req = req.header("X-Odoo-Database", &self.database);
        }
        let mut resp = req
            .send_json(&args)
            .map_err(|e| format!("Odoo does not answer ({e})."))?;
        let status = resp.status().as_u16();
        let body: Value = resp
            .body_mut()
            .read_json()
            .map_err(|e| format!("Odoo's answer is not JSON (HTTP {status}): {e}"))?;
        if status == 200 {
            return Ok(body);
        }
        let msg = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error")
            .to_string();
        Err(match status {
            401 => format!("Odoo refuses the API key ({msg})."),
            403 => format!("The Odoo user may not do this ({msg})."),
            _ => format!("Odoo: {msg} (HTTP {status})."),
        })
    }

    fn search_read(
        &self,
        model: &str,
        domain: Value,
        fields: &[&str],
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let v = self.call(
            model,
            "search_read",
            json!({ "domain": domain, "fields": fields, "limit": limit }),
        )?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }
}

/// A label meaning the old customer number: "Acomba", "Ancien numéro", "Old number".
fn means_old_number(label: &str) -> bool {
    let l = crate::customers::fold(label);
    l.contains("acomba")
        || l.contains("fortune")
        || (l.contains("ancien")
            && (l.contains("numero") || l.contains("no") || l.contains("code")))
        || (l.contains("old")
            && (l.contains("number") || l.contains("code") || l.contains("account")))
        || l.contains("legacy")
}

impl Odoo {
    /// The partner field holding the old customer number: the one in the settings, else a
    /// custom text field labelled like it (Odoo Studio names them x_studio_…).
    fn old_number_field(&self) -> Option<String> {
        if let Some(f) = &self.old_number_field {
            return Some(f.clone());
        }
        let place = format!("{}|{}", self.base, self.database);
        if let Ok(cache) = OLD_FIELDS.lock() {
            if let Some((_, f, at)) = cache.iter().find(|(p, _, _)| *p == place) {
                if at.elapsed() < Duration::from_secs(3600) {
                    return f.clone();
                }
            }
        }
        let fields = self
            .call(
                "res.partner",
                "fields_get",
                json!({ "attributes": ["string", "type"] }),
            )
            .ok()?;
        let found = fields.as_object().and_then(|m| {
            let mut names: Vec<&String> = m
                .iter()
                .filter(|(k, v)| {
                    k.starts_with("x_")
                        && v.get("type").and_then(Value::as_str) == Some("char")
                        && v.get("string")
                            .and_then(Value::as_str)
                            .is_some_and(means_old_number)
                })
                .map(|(k, _)| k)
                .collect();
            names.sort();
            names.first().map(|k| (*k).clone())
        });
        if let Ok(mut cache) = OLD_FIELDS.lock() {
            cache.retain(|(p, _, _)| *p != place);
            cache.push((place, found.clone(), std::time::Instant::now()));
        }
        found
    }

    /// Partners as customers, by domain.
    fn partners(&self, domain: Value, limit: usize) -> Result<Vec<RemoteCustomer>, String> {
        let old = self.old_number_field();
        let mut fields = vec!["id", "name", "ref", "customer_rank"];
        if let Some(f) = &old {
            fields.push(f);
        }
        let text = |r: &Value, k: &str| {
            r.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        };
        Ok(self
            .search_read("res.partner", domain, &fields, limit)?
            .iter()
            .filter_map(|r| {
                let id = r.get("id")?.as_i64()?;
                let name = text(r, "name");
                Some(RemoteCustomer {
                    remote: remote("res.partner", (id, name.clone())),
                    name,
                    old_number: old.as_deref().map(|f| text(r, f)).unwrap_or_default(),
                    // imported records carry a technical reference, not one for people
                    reference: Some(text(r, "ref"))
                        .filter(|x| !x.starts_with("__"))
                        .unwrap_or_default(),
                    rank: r.get("customer_rank").and_then(Value::as_i64).unwrap_or(0),
                })
            })
            .collect())
    }
}

/// The attribute colourways are variants of.
const COLOURWAY: &str = "Colourway";

impl Odoo {
    /// The attribute of this name (making variants), made if missing.
    fn attribute(&self, name: &str) -> Result<i64, String> {
        let found = self.search_read(
            "product.attribute",
            json!([["name", "=", name]]),
            &["id"],
            1,
        )?;
        if let Some(id) = found
            .first()
            .and_then(|c| c.get("id"))
            .and_then(Value::as_i64)
        {
            return Ok(id);
        }
        self.call(
            "product.attribute",
            "create",
            json!({ "vals_list": [{ "name": name, "create_variant": "always", "display_type": "radio" }] }),
        )?
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_i64)
        .ok_or_else(|| "Odoo did not make the attribute.".to_string())
    }

    /// The value of this name of an attribute, made if missing.
    fn attribute_value(&self, attribute: i64, name: &str) -> Result<i64, String> {
        let found = self.search_read(
            "product.attribute.value",
            json!([["attribute_id", "=", attribute], ["name", "=", name]]),
            &["id"],
            1,
        )?;
        if let Some(id) = found
            .first()
            .and_then(|c| c.get("id"))
            .and_then(Value::as_i64)
        {
            return Ok(id);
        }
        self.call(
            "product.attribute.value",
            "create",
            json!({ "vals_list": [{ "attribute_id": attribute, "name": name }] }),
        )?
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_i64)
        .ok_or_else(|| "Odoo did not make the attribute value.".to_string())
    }

    /// Nomenclature lines using product `old` use product `new` instead.
    fn repoint(&self, old: i64, new: i64) -> Result<(), String> {
        let lines: Vec<i64> = self
            .search_read(
                "mrp.bom.line",
                json!([["product_id", "=", old]]),
                &["id"],
                1000,
            )?
            .iter()
            .filter_map(|l| l.get("id")?.as_i64())
            .collect();
        if !lines.is_empty() {
            self.call(
                "mrp.bom.line",
                "write",
                json!({ "ids": lines, "vals": { "product_id": new } }),
            )?;
        }
        Ok(())
    }

    /// The product category of this name, made if missing.
    fn category(&self, name: &str) -> Result<i64, String> {
        let found =
            self.search_read("product.category", json!([["name", "=", name]]), &["id"], 1)?;
        if let Some(id) = found
            .first()
            .and_then(|c| c.get("id"))
            .and_then(Value::as_i64)
        {
            return Ok(id);
        }
        self.call(
            "product.category",
            "create",
            json!({ "vals_list": [{ "name": name }] }),
        )?
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_i64)
        .ok_or_else(|| "Odoo did not make the product category.".to_string())
    }
}

/// Standard base64, for pictures sent to Odoo.
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Odoo's `[id, "name"]` for a many2one, or `false`.
fn m2o(v: &Value) -> Option<(i64, String)> {
    let a = v.as_array()?;
    Some((a.first()?.as_i64()?, a.get(1)?.as_str()?.to_string()))
}

fn remote(model: &str, (id, label): (i64, String)) -> Remote {
    Remote {
        model: model.into(),
        id: id.to_string(),
        label,
    }
}

/// Odoo's UTC date and time ("2026-11-18 14:15:50") as seconds since 1970.
pub fn parse_time(s: &str) -> Option<i64> {
    let (d, t) = s.split_once(' ').unwrap_or((s, "00:00:00"));
    let mut dp = d.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (dp.next()??, dp.next()??, dp.next()??);
    let mut tp = t.split(':').map(|x| x.parse::<f64>().ok());
    let (hh, mm, ss) = (
        tp.next()??,
        tp.next().flatten().unwrap_or(0.0),
        tp.next().flatten().unwrap_or(0.0),
    );
    // days from the civil date (Howard Hinnant's algorithm)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + (hh * 3600.0 + mm * 60.0 + ss) as i64)
}

/// Seconds since 1970 as Odoo's UTC date and time.
pub fn format_time(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn ids(list: &[String]) -> Vec<i64> {
    list.iter().filter_map(|s| s.parse().ok()).collect()
}

impl Connector for Odoo {
    fn test(&mut self) -> Result<String, String> {
        let ctx = self.call("res.users", "context_get", json!({}))?;
        let uid = ctx.get("uid").and_then(Value::as_i64);
        let who = match uid {
            Some(uid) => self
                .search_read(
                    "res.users",
                    json!([["id", "=", uid]]),
                    &["name", "login"],
                    1,
                )?
                .first()
                .and_then(|u| u.get("name").and_then(Value::as_str).map(String::from))
                .unwrap_or_else(|| format!("user {uid}")),
            None => "the API key's user".into(),
        };
        let centers = self.call("mrp.workcenter", "search_count", json!({ "domain": [] }))?;
        Ok(format!(
            "Connected to {} as {who}; {} work centers.",
            self.base.trim_start_matches("https://"),
            centers
        ))
    }

    fn machines(&mut self) -> Result<Vec<Remote>, String> {
        Ok(self
            .search_read("mrp.workcenter", json!([]), &["id", "name", "code"], 500)?
            .iter()
            .filter_map(|w| {
                let id = w.get("id")?.as_i64()?;
                let name = w.get("name")?.as_str()?.to_string();
                let label = match w.get("code").and_then(Value::as_str) {
                    Some(c) if !c.is_empty() => format!("{name} ({c})"),
                    _ => name,
                };
                Some(remote("mrp.workcenter", (id, label)))
            })
            .collect())
    }

    fn design_sources(&mut self, machines: &[String]) -> Result<Vec<Remote>, String> {
        Ok(self
            .search_read(
                "mrp.routing.workcenter",
                json!([["workcenter_id", "in", ids(machines)]]),
                &["id", "name", "bom_id"],
                2000,
            )?
            .iter()
            .filter_map(|o| {
                let id = o.get("id")?.as_i64()?;
                let name = o.get("name")?.as_str()?.to_string();
                let label = match m2o(o.get("bom_id")?) {
                    Some((_, bom)) => format!("{name} — {bom}"),
                    None => name,
                };
                Some(remote("mrp.routing.workcenter", (id, label)))
            })
            .collect())
    }

    fn work(&mut self, machines: &[String], since: Option<i64>) -> Result<Vec<Work>, String> {
        let open = json!(["state", "not in", ["done", "cancel"]]);
        let domain = match since {
            Some(t) => json!([
                ["workcenter_id", "in", ids(machines)],
                "|",
                open,
                ["write_date", ">=", format_time(t)]
            ]),
            None => json!([["workcenter_id", "in", ids(machines)], open]),
        };
        let wos = self.search_read(
            "mrp.workorder",
            domain,
            &[
                "id",
                "name",
                "state",
                "production_id",
                "operation_id",
                "workcenter_id",
                "qty_production",
                "date_start",
                "product_id",
            ],
            2000,
        )?;
        // the MOs: deadline and where they came from
        let mo_ids: Vec<i64> = wos
            .iter()
            .filter_map(|w| m2o(w.get("production_id")?).map(|(id, _)| id))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mos = if mo_ids.is_empty() {
            Vec::new()
        } else {
            self.search_read(
                "mrp.production",
                json!([["id", "in", mo_ids]]),
                &["id", "name", "origin", "date_deadline"],
                mo_ids.len(),
            )?
        };
        // customers from the sale orders named as the MOs' origin (left empty if the user may
        // not read sale orders)
        let origins: Vec<String> = mos
            .iter()
            .filter_map(|m| m.get("origin")?.as_str().map(String::from))
            .collect();
        let orders = if origins.is_empty() {
            Vec::new()
        } else {
            self.search_read(
                "sale.order",
                json!([["name", "in", origins]]),
                &["name", "partner_id", "commercial_partner_id"],
                origins.len(),
            )
            .unwrap_or_default()
        };
        // order name, the customer's name as on the order, and its company
        let customers: Vec<(String, String, Option<i64>)> = orders
            .iter()
            .filter_map(|s| {
                Some((
                    s.get("name")?.as_str()?.to_string(),
                    m2o(s.get("partner_id")?)?.1,
                    s.get("commercial_partner_id").and_then(m2o).map(|x| x.0),
                ))
            })
            .collect();
        let company_ids: Vec<i64> = customers
            .iter()
            .filter_map(|c| c.2)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let companies = if company_ids.is_empty() {
            Vec::new()
        } else {
            self.partners(json!([["id", "in", company_ids]]), company_ids.len())
                .unwrap_or_default()
        };
        // the design components of the MOs (`EMB-` products), with the operation each is
        // consumed in: with variants, an MO only has the component of its own variant
        let moves = if mo_ids.is_empty() {
            Vec::new()
        } else {
            self.search_read(
                "stock.move",
                json!([
                    ["raw_material_production_id", "in", mo_ids],
                    ["product_id.default_code", "=like", "EMB-%"],
                    ["state", "!=", "cancel"]
                ]),
                &["raw_material_production_id", "product_id", "operation_id"],
                5000,
            )?
        };
        let chosen_for = |mo: i64, op: Option<i64>| -> Option<Remote> {
            let of_mo: Vec<&Value> = moves
                .iter()
                .filter(|m| {
                    m.get("raw_material_production_id")
                        .and_then(m2o)
                        .map(|x| x.0)
                        == Some(mo)
                })
                .collect();
            let consumed_here = of_mo
                .iter()
                .find(|m| op.is_some() && m.get("operation_id").and_then(m2o).map(|x| x.0) == op);
            let only = (of_mo.len() == 1).then(|| of_mo[0]);
            consumed_here
                .copied()
                .or(only)
                .and_then(|m| m.get("product_id").and_then(m2o))
                .map(|x| remote("product.product", x))
        };
        let mut out = Vec::new();
        for w in &wos {
            let Some(id) = w.get("id").and_then(Value::as_i64) else {
                continue;
            };
            let name = w.get("name").and_then(Value::as_str).unwrap_or_default();
            let mo = w.get("production_id").and_then(m2o);
            let mo_row = mo.as_ref().and_then(|(mid, _)| {
                mos.iter()
                    .find(|m| m.get("id").and_then(Value::as_i64) == Some(*mid))
            });
            let origin = mo_row
                .and_then(|m| m.get("origin")?.as_str())
                .unwrap_or_default();
            let state = match w.get("state").and_then(Value::as_str).unwrap_or_default() {
                "progress" => WorkState::Started,
                "done" => WorkState::Done,
                "cancel" => WorkState::Cancelled,
                _ => WorkState::Waiting,
            };
            out.push(Work {
                remote: remote("mrp.workorder", (id, name.to_string())),
                reference: match &mo {
                    Some((_, n)) => format!("{n} · {name}"),
                    None => name.to_string(),
                },
                product: w
                    .get("product_id")
                    .and_then(m2o)
                    .map(|p| p.1)
                    .unwrap_or_default(),
                state,
                machine: w
                    .get("workcenter_id")
                    .and_then(m2o)
                    .map(|x| remote("mrp.workcenter", x)),
                design: w
                    .get("operation_id")
                    .and_then(m2o)
                    .map(|x| remote("mrp.routing.workcenter", x)),
                quantity: w
                    .get("qty_production")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0),
                customer: customers
                    .iter()
                    .find(|(so, _, _)| so == origin)
                    .map(|(_, c, _)| c.clone())
                    .unwrap_or_default(),
                client: customers.iter().find(|(so, _, _)| so == origin).and_then(
                    |(_, _, company)| {
                        let id = company.as_ref()?.to_string();
                        companies.iter().find(|c| c.remote.id == id).cloned()
                    },
                ),
                due: mo_row
                    .and_then(|m| m.get("date_deadline")?.as_str())
                    .and_then(parse_time),
                started: w
                    .get("date_start")
                    .and_then(Value::as_str)
                    .and_then(parse_time),
                chosen: mo.as_ref().and_then(|(mid, _)| {
                    chosen_for(*mid, w.get("operation_id").and_then(m2o).map(|x| x.0))
                }),
                note_on: mo.map(|x| remote("mrp.production", x)),
            });
        }
        Ok(out)
    }

    fn customers(&mut self) -> Result<Vec<RemoteCustomer>, String> {
        // companies and people that buy, not their contacts and addresses, nor suppliers
        self.partners(
            json!([
                ["parent_id", "=", false],
                "|",
                ["customer_rank", ">", 0],
                "&",
                ["is_company", "=", true],
                ["supplier_rank", "=", 0]
            ]),
            20_000,
        )
    }

    fn publish(&mut self, p: &Publish, existing: Option<&Remote>) -> Result<Published, String> {
        let category = self.category("Embroidery designs")?;
        let mut vals = json!({
            "name": p.name,
            "type": "consu",
            "categ_id": category,
            "sale_ok": false,
            "purchase_ok": false,
            "list_price": p.price,
            "description": p.description.replace('\n', "<br/>"),
        });
        if let Some(png) = &p.picture {
            vals["image_1920"] = Value::String(base64(png));
        }
        // the product made before: by its link, or by the reference of one of its variants
        let mut template = None;
        if let Some(id) = existing.and_then(|r| r.id.parse::<i64>().ok()) {
            if !self
                .search_read("product.template", json!([["id", "=", id]]), &["id"], 1)?
                .is_empty()
            {
                template = Some(id);
            }
        }
        if template.is_none() {
            template = self
                .search_read(
                    "product.product",
                    json!([
                        "|",
                        ["default_code", "=", p.code],
                        ["default_code", "=like", format!("{}-%", p.code)]
                    ]),
                    &["product_tmpl_id"],
                    1,
                )?
                .first()
                .and_then(|r| m2o(r.get("product_tmpl_id")?))
                .map(|(id, _)| id);
        }
        let template = match template {
            Some(id) => {
                self.call(
                    "product.template",
                    "write",
                    json!({ "ids": [id], "vals": vals }),
                )?;
                id
            }
            None => self
                .call("product.template", "create", json!({ "vals_list": [vals] }))?
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_i64)
                .ok_or("Odoo did not say which product it made.")?,
        };

        // the variants before, to move nomenclatures off any that Odoo replaces
        let before: Vec<(i64, String)> = self
            .search_read(
                "product.product",
                json!([["product_tmpl_id", "=", template]]),
                &["id", "default_code"],
                500,
            )?
            .iter()
            .filter_map(|r| {
                Some((
                    r.get("id")?.as_i64()?,
                    r.get("default_code")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ))
            })
            .collect();
        // colourways as values of a "Colourway" attribute: Odoo makes (or archives) the variants.
        // Always there, even with just the main colours: adding a colourway later then adds a
        // variant and keeps the main colours' one (nomenclatures using it stay as they are)
        let line = self
            .search_read(
                "product.template.attribute.line",
                json!([
                    ["product_tmpl_id", "=", template],
                    ["attribute_id.name", "=", COLOURWAY]
                ]),
                &["id"],
                1,
            )?
            .first()
            .and_then(|l| l.get("id").and_then(Value::as_i64));
        {
            let attribute = self.attribute(COLOURWAY)?;
            let mut values = Vec::new();
            for v in &p.variants {
                values.push(self.attribute_value(attribute, &v.label)?);
            }
            match line {
                Some(l) => {
                    self.call(
                        "product.template.attribute.line",
                        "write",
                        json!({ "ids": [l], "vals": { "value_ids": [[6, 0, values]] } }),
                    )?;
                }
                None => {
                    self.call(
                        "product.template",
                        "write",
                        json!({ "ids": [template], "vals": { "attribute_line_ids": [[0, 0, {
                            "attribute_id": attribute, "value_ids": [[6, 0, values]]
                        }]] } }),
                    )?;
                }
            }
        }

        // each variant: which colours it is, its reference, picture and cost
        let products = self.search_read(
            "product.product",
            json!([["product_tmpl_id", "=", template]]),
            &["id", "product_template_attribute_value_ids"],
            500,
        )?;
        let ptav_ids: Vec<i64> = products
            .iter()
            .filter_map(|r| {
                r.get("product_template_attribute_value_ids")?
                    .as_array()
                    .cloned()
            })
            .flatten()
            .filter_map(|v| v.as_i64())
            .collect();
        let ptavs = if ptav_ids.is_empty() {
            Vec::new()
        } else {
            self.search_read(
                "product.template.attribute.value",
                json!([["id", "in", ptav_ids]]),
                &["id", "name"],
                ptav_ids.len(),
            )?
        };
        let name_of = |id: i64| {
            ptavs
                .iter()
                .find(|x| x.get("id").and_then(Value::as_i64) == Some(id))
                .and_then(|x| x.get("name")?.as_str())
                .map(String::from)
        };
        let mut out = Vec::new();
        for r in &products {
            let Some(id) = r.get("id").and_then(Value::as_i64) else {
                continue;
            };
            let names: Vec<String> = r
                .get("product_template_attribute_value_ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_i64)
                        .filter_map(name_of)
                        .collect()
                })
                .unwrap_or_default();
            // a product without colourways has one variant: the main colours
            let v = if names.is_empty() {
                p.variants.first()
            } else {
                p.variants
                    .iter()
                    .find(|v| names.iter().any(|n| n == &v.label))
            };
            let Some(v) = v else {
                continue;
            };
            let mut vv = json!({ "default_code": v.code, "standard_price": p.cost });
            if p.variants.len() > 1 {
                if let Some(png) = &v.picture {
                    vv["image_variant_1920"] = Value::String(base64(png));
                }
            }
            self.call(
                "product.product",
                "write",
                json!({ "ids": [id], "vals": vv }),
            )?;
            out.push((
                v.colourway,
                Remote {
                    model: "product.product".into(),
                    id: id.to_string(),
                    label: format!("[{}] {} ({})", v.code, p.name, v.label),
                },
            ));
        }
        // nomenclatures using a variant Odoo replaced use the new one of the same colours
        let new_id = |code: &str| {
            out.iter()
                .find(|(_, r)| r.label.starts_with(&format!("[{code}]")))
                .and_then(|(_, r)| r.id.parse::<i64>().ok())
        };
        for (old, code) in &before {
            if out.iter().any(|(_, r)| r.id == old.to_string()) {
                continue;
            }
            let code = if code.is_empty() {
                p.code.as_str()
            } else {
                code.as_str()
            };
            if let Some(n) = new_id(code) {
                self.repoint(*old, n)?;
            }
        }
        // single products made by an earlier version for a colourway, now variants: archived
        let others = self.search_read(
            "product.product",
            json!([
                ["default_code", "=like", format!("{}-%", p.code)],
                ["product_tmpl_id", "!=", template]
            ]),
            &["id", "default_code"],
            100,
        )?;
        let old: Vec<i64> = others
            .iter()
            .filter_map(|r| r.get("id")?.as_i64())
            .collect();
        for r in &others {
            let (Some(id), Some(code)) = (
                r.get("id").and_then(Value::as_i64),
                r.get("default_code").and_then(Value::as_str),
            ) else {
                continue;
            };
            if let Some(n) = new_id(code) {
                self.repoint(id, n)?;
            }
        }
        if !old.is_empty() {
            self.call(
                "product.product",
                "write",
                json!({ "ids": old, "vals": { "active": false } }),
            )?;
        }
        Ok(Published {
            product: Remote {
                model: "product.template".into(),
                id: template.to_string(),
                label: format!("[{}] {}", p.code, p.name),
            },
            variants: out,
        })
    }

    fn design_components(
        &mut self,
        operations: &[String],
    ) -> Result<Vec<(String, Remote)>, String> {
        Ok(self
            .search_read(
                "mrp.bom.line",
                json!([
                    ["operation_id", "in", ids(operations)],
                    ["product_id.default_code", "=like", "EMB-%"]
                ]),
                &["operation_id", "product_id"],
                5000,
            )?
            .iter()
            .filter_map(|l| {
                let (op, _) = m2o(l.get("operation_id")?)?;
                let product = m2o(l.get("product_id")?)?;
                Some((op.to_string(), remote("product.product", product)))
            })
            .collect())
    }

    fn note(&mut self, on: &Remote, text: &str) -> Result<(), String> {
        let id: i64 = on.id.parse().map_err(|_| "bad record id".to_string())?;
        self.call(
            &on.model,
            "message_post",
            json!({
                "ids": [id],
                "body": text,
                "message_type": "comment",
                "subtype_xmlid": "mail.mt_note"
            }),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odoo_times() {
        let t = parse_time("2026-11-18 14:15:50").unwrap_or_default();
        assert_eq!(format_time(t), "2026-11-18 14:15:50");
        assert_eq!(parse_time("1970-01-01 00:00:00"), Some(0));
        assert_eq!(format_time(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(parse_time("2000-03-01 00:00:00"), Some(951_868_800));
        assert_eq!(parse_time("nonsense"), None);
    }

    #[test]
    fn base64_like_everyone() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd, 0x00]), "//79AA==");
    }

    #[test]
    fn settings_are_checked() {
        assert!(Odoo::new(&json!({}), "k").is_err());
        assert!(Odoo::new(&json!({"url": "odoo.example.com"}), "").is_err());
        let o =
            Odoo::new(&json!({"url": "odoo.example.com/"}), "k").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(o.base, "https://odoo.example.com");
    }

    /// The customers of a real Odoo matched to a library's (a copy of its database):
    /// `ODOO_ENV=/path/.env LIBRARY_DB=/copy/library.db cargo test -p embcraft-server live_customers -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_customers() {
        let (Ok(path), Ok(lib)) = (std::env::var("ODOO_ENV"), std::env::var("LIBRARY_DB")) else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let get = |k: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("{k}=")))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let mut o = Odoo::new(&json!({ "url": get("ODOO_URL") }), &get("ODOO_APIKEY"))
            .unwrap_or_else(|e| panic!("{e}"));
        println!("old number field: {:?}", o.old_number_field());
        let remote = o.customers().unwrap_or_else(|e| panic!("{e}"));
        println!(
            "{} customers, {} with an old number",
            remote.len(),
            remote.iter().filter(|r| !r.old_number.is_empty()).count()
        );
        let db = crate::db::open(std::path::Path::new(&lib)).unwrap();
        let m = crate::connections::match_customers(&db, 999, &remote).unwrap();
        let (mut sure, mut maybe, mut none) = (0, 0, 0);
        for x in &m {
            if let Some(r) = &x.sure {
                sure += 1;
                println!(
                    "SURE  {:<45} = {} [{}] ({})",
                    crate::customers::label(&x.customer.name, &x.customer.old_number),
                    r.name,
                    r.old_number,
                    x.how
                );
            } else if !x.maybe.is_empty() {
                maybe += 1;
                println!(
                    "MAYBE {:<45} ? {}",
                    x.customer.name,
                    x.maybe
                        .iter()
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            } else {
                none += 1;
                println!(
                    "NONE  {}",
                    crate::customers::label(&x.customer.name, &x.customer.old_number)
                );
            }
        }
        println!(
            "{} library customers: {sure} sure, {maybe} maybe, {none} none",
            m.len()
        );
    }

    /// Against a real Odoo: `ODOO_ENV=/path/.env cargo test -p embcraft-server live_odoo -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_odoo() {
        let publish_test = std::env::var("ODOO_PUBLISH_TEST").is_ok();
        let Ok(path) = std::env::var("ODOO_ENV") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let get = |k: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("{k}=")))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let mut o = Odoo::new(&json!({ "url": get("ODOO_URL") }), &get("ODOO_APIKEY"))
            .unwrap_or_else(|e| panic!("{e}"));
        println!("{}", o.test().unwrap_or_else(|e| panic!("{e}")));
        let m = o.machines().unwrap_or_else(|e| panic!("{e}"));
        println!(
            "{} work centers: {:?}",
            m.len(),
            m.iter().map(|r| &r.label).collect::<Vec<_>>()
        );
        let d = o
            .design_sources(&["6".into()])
            .unwrap_or_else(|e| panic!("{e}"));
        println!(
            "operations on Tajima 1: {:?}",
            d.iter().map(|r| &r.label).collect::<Vec<_>>()
        );
        let w = o
            .work(&["6".into(), "1".into()], None)
            .unwrap_or_else(|e| panic!("{e}"));
        println!("{} open work orders on Tajima 1 + Embroidery", w.len());
        if publish_test {
            let variant = |cw: Option<i64>, label: &str, code: &str| crate::connections::Variant {
                colourway: cw,
                label: label.into(),
                code: code.into(),
                picture: None,
            };
            let mut p = Publish {
                name: "EmbroideryCraft test design".into(),
                code: "EMB-TEST2".into(),
                description: "670 stitches\nFrom the EmbroideryCraft Library.".into(),
                picture: None,
                cost: 1.23,
                price: 4.56,
                variants: vec![variant(None, "Main colours", "EMB-TEST2")],
            };
            let r = o.publish(&p, None).unwrap_or_else(|e| panic!("{e}"));
            println!("published plain: {:?}", r.variants);
            p.variants.push(variant(Some(1), "Noir", "EMB-TEST2-1"));
            p.variants.push(variant(Some(2), "Blanc", "EMB-TEST2-2"));
            let r2 = o
                .publish(&p, Some(&r.product))
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(r2.product.id, r.product.id, "the same product");
            println!("with colourways: {:?}", r2.variants);
            assert_eq!(r2.variants.len(), 3);
            assert!(
                r2.variants
                    .iter()
                    .any(|(cw, v)| cw.is_none() && v.id == r.variants[0].1.id),
                "the main colours keep their variant"
            );
            p.variants.pop();
            let r3 = o
                .publish(&p, Some(&r.product))
                .unwrap_or_else(|e| panic!("{e}"));
            println!("one colourway removed: {:?}", r3.variants);
            assert_eq!(r3.variants.len(), 2);
            println!("product {}", r.product.id);
            let c = o
                .design_components(&["12".into(), "13".into()])
                .unwrap_or_else(|e| panic!("{e}"));
            println!("components consumed in operations 12, 13: {c:?}");
        }
        for x in w.iter().take(3) {
            println!(
                "  {} | {} | {:?} | {:?} | qty {} | {} | due {:?}",
                x.reference,
                x.product,
                x.state,
                x.design.as_ref().map(|d| &d.label),
                x.quantity,
                x.customer,
                x.due.map(format_time)
            );
        }
    }
}
