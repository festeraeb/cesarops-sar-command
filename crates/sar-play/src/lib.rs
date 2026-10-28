//! Mission plays: templates of roles, checklist and ICS structure.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IcsSection {
    Command,
    Operations,
    Planning,
    Logistics,
    Finance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub title: String,
    pub ics_section: IcsSection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub text: String,
    pub role_id: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Play {
    pub id: String,
    pub domain: String,
    pub roles: Vec<Role>,
    pub checklist: Vec<ChecklistItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayError {
    Unknown(String),
    DuplicateRole(String),
    DuplicateItem(String),
    UnknownRole { item_id: String, role_id: String },
    UnknownItem(String),
}

impl fmt::Display for PlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "unknown play id {id:?}"),
            Self::DuplicateRole(id) => write!(f, "duplicate role id {id:?}"),
            Self::DuplicateItem(id) => write!(f, "duplicate checklist item id {id:?}"),
            Self::UnknownRole { item_id, role_id } => {
                write!(f, "checklist item {item_id:?} names unknown role {role_id:?}")
            }
            Self::UnknownItem(id) => write!(f, "unknown checklist item {id:?}"),
        }
    }
}

impl std::error::Error for PlayError {}

fn role(id: &str, title: &str, ics_section: IcsSection) -> Role {
    Role { id: id.into(), title: title.into(), ics_section }
}

pub fn water_vessel_sar() -> Play {
    use IcsSection::*;
    let roles = vec![
        role("incident_commander", "Incident Commander", Command),
        role("operations_chief", "Operations Section Chief", Operations),
        role("planning_chief", "Planning Section Chief", Planning),
        role("logistics_chief", "Logistics Section Chief", Logistics),
        role("family_liaison", "Family Liaison", Command),
        role("marine_unit_leader", "Marine Unit Leader", Operations),
        role("aviation_coordinator", "Aviation Coordinator", Operations),
    ];
    let items: [(&str, &str, &str); 14] = [
        ("confirm_lkp", "Confirm last known position and time, and record who reported them", "incident_commander"),
        ("notify_rcc", "Notify the Coast Guard rescue coordination centre", "incident_commander"),
        ("vessel_description", "Collect vessel description: type, length, colour, registration, radio and beacon details", "planning_chief"),
        ("persons_on_board", "Collect the number and names of persons on board, with age, medical conditions and clothing", "planning_chief"),
        ("check_ais_adsb", "Check AIS and ADS-B history for the vessel and for aircraft near the area", "planning_chief"),
        ("record_weather", "Record weather, sea state and water temperature at last known time and now", "planning_chief"),
        ("search_area_drift", "Establish the search area and the drift inputs: wind, current, leeway", "planning_chief"),
        ("family_cadence", "Set the family contact cadence and agree who speaks to the family", "family_liaison"),
        ("start_updates_log", "Start the updates log and record every decision with time and role", "incident_commander"),
        ("assign_marine_units", "Assign marine search units to search sectors and record their callsigns", "marine_unit_leader"),
        ("request_aviation", "Request or confirm aerial search assets and agree flight coordination", "aviation_coordinator"),
        ("fuel_and_crews", "Confirm fuel, crew rest and relief plan for all search units", "logistics_chief"),
        ("comms_plan", "Set the radio channels and check-in schedule for all units", "operations_chief"),
        ("brief_units", "Brief all units on the search area, subject details and reporting rules", "operations_chief"),
    ];
    let checklist = items
        .iter()
        .map(|(id, text, role_id)| ChecklistItem {
            id: (*id).into(),
            text: (*text).into(),
            role_id: (*role_id).into(),
            done: false,
        })
        .collect();
    Play { id: "water_vessel_sar".into(), domain: "water".into(), roles, checklist }
}

pub fn play_by_id(id: &str) -> Result<Play, PlayError> {
    match id {
        "water_vessel_sar" => Ok(water_vessel_sar()),
        other => Err(PlayError::Unknown(other.to_string())),
    }
}

impl Play {
    pub fn validate(&self) -> Result<(), PlayError> {
        let mut roles = HashSet::new();
        for r in &self.roles {
            if !roles.insert(r.id.as_str()) {
                return Err(PlayError::DuplicateRole(r.id.clone()));
            }
        }
        let mut items = HashSet::new();
        for i in &self.checklist {
            if !items.insert(i.id.as_str()) {
                return Err(PlayError::DuplicateItem(i.id.clone()));
            }
            if !roles.contains(i.role_id.as_str()) {
                return Err(PlayError::UnknownRole { item_id: i.id.clone(), role_id: i.role_id.clone() });
            }
        }
        Ok(())
    }

    pub fn set_done(&mut self, item_id: &str, done: bool) -> Result<(), PlayError> {
        match self.checklist.iter_mut().find(|i| i.id == item_id) {
            Some(item) => {
                item.done = done;
                Ok(())
            }
            None => Err(PlayError::UnknownItem(item_id.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_vessel_sar_validates() {
        assert_eq!(water_vessel_sar().validate(), Ok(()));
        assert_eq!(water_vessel_sar().id, "water_vessel_sar");
        assert_eq!(water_vessel_sar().domain, "water");
    }

    #[test]
    fn has_seven_roles_and_enough_items() {
        let p = water_vessel_sar();
        for id in [
            "incident_commander", "operations_chief", "planning_chief", "logistics_chief",
            "family_liaison", "marine_unit_leader", "aviation_coordinator",
        ] {
            assert!(p.roles.iter().any(|r| r.id == id), "{id}");
        }
        assert_eq!(p.roles.len(), 7);
        assert!(p.checklist.len() >= 12);
        assert_eq!(p.roles[0].ics_section, IcsSection::Command);
        assert!(p.checklist.iter().all(|i| !i.done));
    }

    #[test]
    fn play_by_id_known_and_unknown() {
        assert_eq!(play_by_id("water_vessel_sar"), Ok(water_vessel_sar()));
        assert_eq!(play_by_id("land_person"), Err(PlayError::Unknown("land_person".into())));
        assert_eq!(play_by_id(""), Err(PlayError::Unknown(String::new())));
    }

    #[test]
    fn duplicate_role_rejected() {
        let mut p = water_vessel_sar();
        p.roles.push(p.roles[0].clone());
        assert_eq!(p.validate(), Err(PlayError::DuplicateRole("incident_commander".into())));
    }

    #[test]
    fn duplicate_item_rejected() {
        let mut p = water_vessel_sar();
        p.checklist.push(p.checklist[0].clone());
        assert_eq!(p.validate(), Err(PlayError::DuplicateItem("confirm_lkp".into())));
    }

    #[test]
    fn bad_role_id_rejected() {
        let mut p = water_vessel_sar();
        p.checklist[3].role_id = "nobody".into();
        assert!(matches!(p.validate(), Err(PlayError::UnknownRole { .. })));
    }

    #[test]
    fn set_done_changes_only_named_item() {
        let mut p = water_vessel_sar();
        let before = p.clone();
        p.set_done("notify_rcc", true).unwrap();
        for (a, b) in before.checklist.iter().zip(&p.checklist) {
            assert_eq!(b.done, b.id == "notify_rcc");
            assert_eq!((&a.id, &a.text, &a.role_id), (&b.id, &b.text, &b.role_id));
        }
        assert_eq!(before.roles, p.roles);
        p.set_done("notify_rcc", false).unwrap();
        assert_eq!(p, before);
    }

    #[test]
    fn set_done_unknown_item_is_err() {
        let mut p = water_vessel_sar();
        assert_eq!(p.set_done("nope", true), Err(PlayError::UnknownItem("nope".into())));
        assert_eq!(p, water_vessel_sar());
    }

    #[test]
    fn json_round_trip() {
        let mut p = water_vessel_sar();
        p.set_done("start_updates_log", true).unwrap();
        let back: Play = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, back);
    }
}
