//! Fictional demo incidents. Made up: no real people, no real cases.
//! Places are real geography (Great Lakes, a river) for a realistic map.

use chrono::{Duration, Utc};
use command_db::{ContactInput, CreateCase, Database, SubjectInput};

pub fn seeded() -> Result<Database, String> {
    let mut db = Database::open_in_memory().map_err(|e| e.to_string())?;
    let now = Utc::now();
    let ts = |mins: i64| (now - Duration::minutes(mins)).to_rfc3339();
    struct Inc<'a> {
        id: &'a str, objective: &'a str, priority: &'a str, subject: &'a str, age: u8, med: &'a [&'a str],
        lkp: (f64, f64), lkp_text: &'a str, lkp_min: i64, contact: (&'a str, &'a str, &'a str),
        updates: &'a [(&'a str, &'a str, i64)], clues: &'a [(&'a str, f64, f64, i64)],
        team: &'a [(&'a str, &'a str, &'a str, &'a str)],
    }
    let incidents = [
        Inc {
            id: "DEMO-LM-001", objective: "vessel_sar", priority: "critical",
            subject: "Demo kayaker (fictional)", age: 34, med: &["none reported"],
            lkp: (43.7505, -87.6890), lkp_text: "Seen paddling 1.5 nm off the Sheboygan breakwater by a fictional charter captain",
            lkp_min: 190, contact: ("Demo family contact", "555-0100", "sibling"),
            updates: &[("Incident Commander", "Wind SW 15 kt, waves 3 ft; search area set 4 nm around the LKP.", 150),
                       ("Marine Unit Leader", "Boat 1 on scene, running creeping-line pattern north of the LKP.", 90)],
            clues: &[("Empty paddle float spotted floating by Boat 1", 43.7810, -87.6550, 70)],
            team: &[("Marine 1", "Demo boat crew A", "Marine Unit Leader", "deployed"),
                    ("Air 2", "Demo helicopter crew", "Air Operations", "assigned")],
        },
        Inc {
            id: "DEMO-MR-002", objective: "person_search", priority: "high",
            subject: "Demo canoeist (fictional)", age: 58, med: &["diabetes"],
            lkp: (44.0521, -91.6393), lkp_text: "Launched at a Mississippi River ramp near Winona; did not return by dusk",
            lkp_min: 320, contact: ("Demo spouse", "555-0101", "spouse"),
            updates: &[("Incident Commander", "River stage rising; backwater sloughs prioritised for the shoreline teams.", 200)],
            clues: &[("Canoe seen drifting empty near a sandbar", 44.0140, -91.5800, 110)],
            team: &[("River 1", "Demo swiftwater team", "Shoreline Team Leader", "assigned"),
                    ("K9 3", "Demo dog team", "Ground Search", "available")],
        },
        Inc {
            id: "DEMO-LS-003", objective: "vessel_sar", priority: "medium",
            subject: "Demo fishing boat, 2 aboard (fictional)", age: 45, med: &[],
            lkp: (46.9050, -90.6500), lkp_text: "Last radio check near the Apostle Islands; vessel overdue by four hours",
            lkp_min: 480, contact: ("Demo harbour master", "555-0102", "reporting party"),
            updates: &[("Watch Officer", "Overdue vessel report received; radio calls on channel 16 unanswered.", 240)],
            clues: &[],
            team: &[("Cutter 5", "Demo cutter crew", "Marine Unit Leader", "assigned")],
        },
    ];
    for i in incidents {
        let case = db
            .create_case(CreateCase {
                id: Some(i.id.into()), domain: "water".into(), objective: i.objective.into(),
                priority: i.priority.into(), play_id: None,
                subject: SubjectInput {
                    name: i.subject.into(), age: Some(i.age), clothing_description: None,
                    medical_conditions: i.med.iter().map(|s| s.to_string()).collect(),
                },
                contacts: vec![ContactInput { name: i.contact.0.into(), phone: Some(i.contact.1.into()), relationship: Some(i.contact.2.into()) }],
            })
            .map_err(|e| e.to_string())?;
        db.add_clue(&case.id, &format!("Last known position: {}", i.lkp_text), "intake", &ts(i.lkp_min), Some(i.lkp))
            .map_err(|e| e.to_string())?;
        for (role, text, m) in i.updates {
            db.add_update(&case.id, text, role, &ts(*m)).map_err(|e| e.to_string())?;
        }
        for (text, lat, lon, m) in i.clues {
            db.add_clue(&case.id, text, "Search team", &ts(*m), Some((*lat, *lon))).map_err(|e| e.to_string())?;
        }
        for (cs, name, role, status) in i.team {
            db.add_team_member(&case.id, &format!("{cs} — {name}"), role, status).map_err(|e| e.to_string())?;
        }
    }
    Ok(db)
}
