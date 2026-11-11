// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! UAS occurrence taxonomy — Annex VI to Implementing Regulation (EU) 2015/1018,
//! as added by Commission Implementing Regulation (EU) 2026/1821.
//!
//! APPLICABILITY (Annex VI, Remark 2 + recitals 6 and 7 of 2026/1821): this list is
//! mandatorily reportable ONLY for UAS operations for which a certificate or a
//! declaration for design is required under Article 56(1) and (5) of Regulation
//! (EU) 2018/1139. Operations that require neither are explicitly EXCLUDED from
//! mandatory reporting. Classification is still offered for every occurrence,
//! because it is equally useful for voluntary reporting under Article 5 of
//! Regulation (EU) No 376/2014 and for the operator's own trend analysis — but the
//! 72-hour reporting deadline must NEVER be presented as an obligation unless
//! applicability has been established. See services/occurrence_reporting.rs.
//!
//! Held as a compile-time constant rather than a database table: this is immutable
//! regulatory text that only changes when the regulation changes, which is a code
//! change in any case. Mirrors the existing VALID_SEVERITIES pattern in
//! handlers/incidents.rs and keeps the taxonomy out of CANONICAL_TABLES, the
//! FKSpec table and the export/import round-trip entirely.
//!
//! Verbatim from the Official Journal: each entry's `text` is the verbatim item
//! text as printed in the Annex. Footnote markers and footnote bodies have been
//! removed, because they attach to the printed page rather than to the item;
//! the two footnote definitions are recorded below. Do not otherwise reword,
//! renumber or "correct" the text — including the source typo in section 7
//! ("OTHER OCCURENCES").
//!
//! Footnotes from the source:
//! - Footnote 1 (referenced from VI.1.1.2) — TMPR: measures used in drone risk
//!   assessments to prevent mid-air collisions by addressing residual air risk.
//! - Footnote 2 (referenced from VI.1.4.3) — CMU (control and monitoring unit):
//!   per Article 3, point (38) of Delegated Regulation (EU) 2019/945, the
//!   equipment to control and monitor unmanned aircraft remotely, as defined in
//!   Article 3, point (32) of Regulation (EU) 2018/1139.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OccurrenceCategory {
    /// Stable identifier, e.g. "VI.1.4.11" or "VI.6.10". Persisted on
    /// `incidents.occurrence_code`; never reassigned.
    pub code: &'static str,
    pub section: u8,
    pub section_title: &'static str,
    /// `None` for sections 3-8, which have no subsections.
    pub subsection: Option<&'static str>,
    pub subsection_title: Option<&'static str>,
    pub text: &'static str,
}

pub const SOURCE: &str = "Annex VI to Implementing Regulation (EU) 2015/1018, as added by Implementing Regulation (EU) 2026/1821";

pub const OCCURRENCE_TAXONOMY: [OccurrenceCategory; 78] = [
    // 1.1 Flight planning and UAS preparation
    OccurrenceCategory {
        code: "VI.1.1.1",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "UAS operation is planned in an operational volume exceeding the limitations described in the issued authorisations or the privileges of the Light UAS operator certificate (LUC).",
    },
    OccurrenceCategory {
        code: "VI.1.1.2",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "Inappropriate planning of the UAS operation and the related mitigation measures, which have or could have endangered any aircraft or any other person (example: contingency and emergency procedures, operational volume, Tactical Mitigation Performance Requirement (TMPR)).",
    },
    OccurrenceCategory {
        code: "VI.1.1.3",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "Use of incorrect data or erroneous entries into the UAS or into equipment used for navigation or performance calculations leading to a malfunction or a failure of the UAS, which has or could have endangered any aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.1.1.4",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "Configuration of the UAS different from the instructions defined or authorised by the manufacturer or by the competent authority (example: payload, software configuration, etc.), which has or could have endangered the unmanned aircraft or any other aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.1.1.5",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "Carriage or attempted carriage of dangerous goods when not in accordance with applicable legislations including incorrect labelling, packaging and handling of dangerous goods.",
    },
    OccurrenceCategory {
        code: "VI.1.1.6",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "Incorrect, unsuitable, or degraded energy source (including fuel).",
    },
    OccurrenceCategory {
        code: "VI.1.1.7",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.1"),
        subsection_title: Some("Flight planning and UAS preparation"),
        text: "Missing, incorrect or inadequate de-icing/ anti-icing treatment.",
    },
    // 1.2 Take-off and landing – Operations from/to aerodromes or vertiports
    OccurrenceCategory {
        code: "VI.1.2.1",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Runway incursions, Final Approach and Take-off Area (FATO) incursion.",
    },
    OccurrenceCategory {
        code: "VI.1.2.2",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Runway excursions.",
    },
    OccurrenceCategory {
        code: "VI.1.2.3",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Any rejected take-off.",
    },
    OccurrenceCategory {
        code: "VI.1.2.4",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Actual or attempted take-off, approach or landing with incorrect configuration setting.",
    },
    OccurrenceCategory {
        code: "VI.1.2.5",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Tail, blade/wingtip or nacelle strike during take-off or landing.",
    },
    OccurrenceCategory {
        code: "VI.1.2.6",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Jet blast or rotor and propellers down/out wash occurrences which have or could have endangered any aircraft, or person.",
    },
    OccurrenceCategory {
        code: "VI.1.2.7",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Hard, short and long landing.",
    },
    OccurrenceCategory {
        code: "VI.1.2.8",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.2"),
        subsection_title: Some("Take-off and landing – Operations from/to aerodromes or vertiports"),
        text: "Foreign Object Damage/Debris (FOD) or poor surface conditions on the take-off and landing surface.",
    },
    // 1.3 Take-off and landing – Operations outside aerodromes or vertiports
    OccurrenceCategory {
        code: "VI.1.3.1",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.3"),
        subsection_title: Some("Take-off and landing – Operations outside aerodromes or vertiports"),
        text: "Taking off, landing outside of the intended landing area.",
    },
    OccurrenceCategory {
        code: "VI.1.3.2",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.3"),
        subsection_title: Some("Take-off and landing – Operations outside aerodromes or vertiports"),
        text: "Inappropriate security and safety measures on the ground area which has or could have endangered any person.",
    },
    // 1.4 Any phase of flight
    OccurrenceCategory {
        code: "VI.1.4.1",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Loss of control of the operation.",
    },
    OccurrenceCategory {
        code: "VI.1.4.2",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Prolonged loss of visual reference for Visual Line-Of-Sight (VLOS) operations (example: due to adverse meteorological conditions or obstacles).",
    },
    OccurrenceCategory {
        code: "VI.1.4.3",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Loss of situational awareness (example: including environmental conditions, mode and system awareness, excessive workload, spatial disorientation, time horizon and improper interaction with the equipment to control the unmanned aircraft remotely (Control and Monitoring Unit (CMU)).",
    },
    OccurrenceCategory {
        code: "VI.1.4.4",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Incorrect fuel or energy management (example: critically low fuel/energy quantity or fuel/energy quantity at destination below required final fuel/energy reserve).",
    },
    OccurrenceCategory {
        code: "VI.1.4.5",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Inability or failure to achieve required aircraft performance expected in normal conditions, during take-off, climb or landing.",
    },
    OccurrenceCategory {
        code: "VI.1.4.6",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Exceedance of aircraft limitations defined by the manufacturer.",
    },
    OccurrenceCategory {
        code: "VI.1.4.7",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Operation with incorrect altimeter setting.",
    },
    OccurrenceCategory {
        code: "VI.1.4.8",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Misinterpretation of UAS automation mode of functioning or of any information from the CMU provided to the remote crew which has or could have endangered any aircraft, or person.",
    },
    OccurrenceCategory {
        code: "VI.1.4.9",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Inability to broadcast the remote identification or aircraft identification.",
    },
    OccurrenceCategory {
        code: "VI.1.4.10",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Unintentional release of cargo or other externally carried equipment.",
    },
    OccurrenceCategory {
        code: "VI.1.4.11",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.4"),
        subsection_title: Some("Any phase of flight"),
        text: "Deviations from the authorised operational volume exceeding the limitations described in the issued operational authorisations or the privileges of the Light UAS operator certificate (LUC) or the authorised flight plan of the UAS, including but not limited to: (a) Unauthorised UAS operation over assemblies of people; (b) Airspace infringement; (c) Geographical zones infringements, where UAS operations are not permitted, or the required flight authorisation was not obtained.",
    },
    // 1.5 Additional occurrences for UAS operations with occupants on board
    OccurrenceCategory {
        code: "VI.1.5.1",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.5"),
        subsection_title: Some("Additional occurrences for UAS operations with occupants on board"),
        text: "Physical distress, incapacitation of the crew, which has or could have endangered the unmanned aircraft or any other aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.1.5.2",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.5"),
        subsection_title: Some("Additional occurrences for UAS operations with occupants on board"),
        text: "Contaminated air in the passenger compartment which has or could have endangered its occupants or any other person.",
    },
    OccurrenceCategory {
        code: "VI.1.5.3",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.5"),
        subsection_title: Some("Additional occurrences for UAS operations with occupants on board"),
        text: "Difficulty in controlling intoxicated, violent or unruly passengers.",
    },
    OccurrenceCategory {
        code: "VI.1.5.4",
        section: 1,
        section_title: "Air Operations",
        subsection: Some("1.5"),
        subsection_title: Some("Additional occurrences for UAS operations with occupants on board"),
        text: "Discovery of a stowaway.",
    },
    // 2.1 Structure and systems
    OccurrenceCategory {
        code: "VI.2.1.1",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Loss of any part of the unmanned aircraft in flight.",
    },
    OccurrenceCategory {
        code: "VI.2.1.2",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Leakage of any fluid which resulted in a fire hazard or possible hazardous contamination of the unmanned aircraft system, or which has or could have endangered the unmanned aircraft, or any other person.",
    },
    OccurrenceCategory {
        code: "VI.2.1.3",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Malfunction or abnormal functioning of flight controls.",
    },
    OccurrenceCategory {
        code: "VI.2.1.4",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Malfunction, loss, or significant degradation of the command-and-control link, including failures affecting its availability, performance, continuity, integrity, or security, which has or could have endangered any aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.2.1.5",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Malfunction of the collision avoidance system.",
    },
    OccurrenceCategory {
        code: "VI.2.1.6",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Malfunction of automatic contingency or emergency system or of the system preventing the unmanned aircraft from exiting the operational volume (example: return home, automatic landing, geo-caging, Flight Termination System).",
    },
    OccurrenceCategory {
        code: "VI.2.1.7",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Any other failure, loss, malfunction, damage of the unmanned aircraft or its systems which has or could have endangered any other aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.2.1.8",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.1"),
        subsection_title: Some("Structure and systems"),
        text: "Any failure, loss, malfunction, damage of the CMU which has or could have endangered the unmanned aircraft or any other aircraft or person.",
    },
    // 2.2 Propulsion (including engines, propellers, and rotor systems) and Auxiliary Power Units (APUs)
    OccurrenceCategory {
        code: "VI.2.2.1",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.2"),
        subsection_title: Some("Propulsion (including engines, propellers, and rotor systems) and Auxiliary Power Units (APUs)"),
        text: "Flameout, in-flight shutdown of any engine, or loss of lift and thrust units or of the APU when required.",
    },
    OccurrenceCategory {
        code: "VI.2.2.2",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.2"),
        subsection_title: Some("Propulsion (including engines, propellers, and rotor systems) and Auxiliary Power Units (APUs)"),
        text: "Engine operating limitation exceedance, including overspeed or inability to control the speed of any high-speed rotating component (example: APU, air starter, air cycle machine, air turbine motor, propeller or rotor).",
    },
    OccurrenceCategory {
        code: "VI.2.2.3",
        section: 2,
        section_title: "Technical Occurrences",
        subsection: Some("2.2"),
        subsection_title: Some("Propulsion (including engines, propellers, and rotor systems) and Auxiliary Power Units (APUs)"),
        text: "Failure or malfunction of any part of an engine, lift and thrust units, APU or transmission resulting in any one or more of the following: (a) Thrust-reversing system failing to operate as commanded; (b) Inability to control power, thrust or rpm (revolutions per minute); (c) Non-containment of components/debris.",
    },
    // 3 Interaction With Atm/Ans
    OccurrenceCategory {
        code: "VI.3.1",
        section: 3,
        section_title: "Interaction With Atm/Ans",
        subsection: None,
        subsection_title: None,
        text: "Unsafe Air Traffic Control (ATC) clearance or instructions.",
    },
    OccurrenceCategory {
        code: "VI.3.2",
        section: 3,
        section_title: "Interaction With Atm/Ans",
        subsection: None,
        subsection_title: None,
        text: "Prolonged loss of communication with Air Traffic Services (ATS) Unit.",
    },
    OccurrenceCategory {
        code: "VI.3.3",
        section: 3,
        section_title: "Interaction With Atm/Ans",
        subsection: None,
        subsection_title: None,
        text: "Conflicting instructions from different ATS Units potentially leading to a loss of separation.",
    },
    OccurrenceCategory {
        code: "VI.3.4",
        section: 3,
        section_title: "Interaction With Atm/Ans",
        subsection: None,
        subsection_title: None,
        text: "Misinterpretation of radio-communication which has or could have endangered the unmanned aircraft or any other aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.3.5",
        section: 3,
        section_title: "Interaction With Atm/Ans",
        subsection: None,
        subsection_title: None,
        text: "Intentional deviation from ATC instruction which has or could have endangered the unmanned aircraft or any other aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.3.6",
        section: 3,
        section_title: "Interaction With Atm/Ans",
        subsection: None,
        subsection_title: None,
        text: "Altitude or flight level bust.",
    },
    // 4 Emergencies And Other Critical Situations
    OccurrenceCategory {
        code: "VI.4.1",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "A collision or a near collision on the ground or in the air, with another aircraft, terrain or obstacle.",
    },
    OccurrenceCategory {
        code: "VI.4.2",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Any emergency manoeuvre taken by the UAS to prevent any collision on the ground or in the air, with another aircraft, terrain or obstacle.",
    },
    OccurrenceCategory {
        code: "VI.4.3",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Any event leading to the declaration of an emergency.",
    },
    OccurrenceCategory {
        code: "VI.4.4",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Activation of an emergency procedure to terminate the flight.",
    },
    OccurrenceCategory {
        code: "VI.4.5",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Failure to apply the correct contingency or emergency procedures.",
    },
    OccurrenceCategory {
        code: "VI.4.6",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Any burning, melting, smoke, fumes, arcing, overheating, fire or explosion on the UAS.",
    },
    OccurrenceCategory {
        code: "VI.4.7",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Any burning, melting, smoke, fumes, arcing, overheating, fire or explosion on the CMU.",
    },
    OccurrenceCategory {
        code: "VI.4.8",
        section: 4,
        section_title: "Emergencies And Other Critical Situations",
        subsection: None,
        subsection_title: None,
        text: "Incapacitation of the remote crew controlling the CMU which has or could have endangered any aircraft or person.",
    },
    // 5 External Environment And Meteorology
    OccurrenceCategory {
        code: "VI.5.1",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "A collision or a near collision on the ground or in the air, with another aircraft, terrain or obstacle.",
    },
    OccurrenceCategory {
        code: "VI.5.2",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Airborne Collision Avoidance System, Resolution Advisory (ACAS RA).",
    },
    OccurrenceCategory {
        code: "VI.5.3",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Activation of genuine ground collision system such as Ground Proximity Warning System (GPWS)/ Terrain Awareness and Warning System (TAWS) ‘warning’.",
    },
    OccurrenceCategory {
        code: "VI.5.4",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Wildlife strike including bird strike which has endangered the unmanned aircraft.",
    },
    OccurrenceCategory {
        code: "VI.5.5",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Foreign Object Damage/Debris (FOD).",
    },
    OccurrenceCategory {
        code: "VI.5.6",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Unexpected encounter of poor runway surface conditions.",
    },
    OccurrenceCategory {
        code: "VI.5.7",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Wake-turbulence encounters which has or could have endangered an aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.5.8",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Interference with the unmanned aircraft by firearms, fireworks, flying kites, laser illumination, high powered lights, lasers, Unmanned Aircraft Systems (UAS), or counter UAS system or by similar means.",
    },
    OccurrenceCategory {
        code: "VI.5.9",
        section: 5,
        section_title: "External Environment And Meteorology",
        subsection: None,
        subsection_title: None,
        text: "Any severe environmental condition which resulted in damage to the aircraft, in handling difficulties or loss or malfunction of any aircraft system (examples: lightning strike, severe turbulence, windshear, icing conditions, volcanic ash, etc.).",
    },
    // 6 U-Space Related Occurrences
    OccurrenceCategory {
        code: "VI.6.1",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "UAS operations inside U-space airspace which are not compliant with the applicable operational conditions and airspace constraints or performed with an unmanned aircraft system not compliant with the required UAS capabilities and performance requirements.",
    },
    OccurrenceCategory {
        code: "VI.6.2",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Missing, incorrect, delayed or misleading information, instruction or service provided by a U-space Service Provider (USSP), which has or could have endangered the unmanned aircraft or any other aircraft or person.",
    },
    OccurrenceCategory {
        code: "VI.6.3",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Non-compliance with the flight authorisation (example: absence of flight authorisation, failure to activate the flight authorisation, deviations of the flight authorisation in space or time).",
    },
    OccurrenceCategory {
        code: "VI.6.4",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Unauthorised exiting of the U-space airspace.",
    },
    OccurrenceCategory {
        code: "VI.6.5",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Improper use of the U-space services leading to loss of situational awareness.",
    },
    OccurrenceCategory {
        code: "VI.6.6",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Inability to execute the Dynamic Airspace Reconfiguration (DAR) procedure.",
    },
    OccurrenceCategory {
        code: "VI.6.7",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Insufficient spacing between the unmanned aircraft and any manned aircraft.",
    },
    OccurrenceCategory {
        code: "VI.6.8",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Encounter of manned aircraft in an active U-space airspace designated in controlled airspace.",
    },
    OccurrenceCategory {
        code: "VI.6.9",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Encounter of a non-conspicuous manned aircraft in U-space airspace designated in uncontrolled airspace.",
    },
    OccurrenceCategory {
        code: "VI.6.10",
        section: 6,
        section_title: "U-Space Related Occurrences",
        subsection: None,
        subsection_title: None,
        text: "Encounter of a non-cooperative unmanned aircraft in the U-space airspace.",
    },
    // 7 Other Occurences
    OccurrenceCategory {
        code: "VI.7.1",
        section: 7,
        section_title: "Other Occurences",
        subsection: None,
        subsection_title: None,
        text: "Any occurrence where the human performance, including fatigue and improper training/qualification of operator’s personnel and inter crew communication, has directly contributed to, or could have contributed to an accident or a serious incident.",
    },
    // 8 Information Security
    OccurrenceCategory {
        code: "VI.8.1",
        section: 8,
        section_title: "Information Security",
        subsection: None,
        subsection_title: None,
        text: "Abnormal behaviour of a system due to an information security incident (example: compromised information or data, malware infection, Distributed-Denial-of-Service (DDoS)).’",
    },
];

/// Look up a category by its stable code. Used to validate
/// `incidents.occurrence_code` on create and update.
pub fn find(code: &str) -> Option<&'static OccurrenceCategory> {
    OCCURRENCE_TAXONOMY.iter().find(|c| c.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in OCCURRENCE_TAXONOMY.iter() {
            assert!(seen.insert(c.code), "duplicate code {}", c.code);
        }
    }

    #[test]
    fn every_entry_is_populated() {
        for c in OCCURRENCE_TAXONOMY.iter() {
            assert!(!c.text.trim().is_empty(), "empty text for {}", c.code);
            assert!(c.code.starts_with("VI."), "unexpected code {}", c.code);
            assert!((1..=8).contains(&c.section), "bad section for {}", c.code);
        }
    }

    #[test]
    fn no_footnote_contamination() {
        // Footnote apparatus belongs to the printed OJ page, not to any item.
        for c in OCCURRENCE_TAXONOMY.iter() {
            for pat in ["OJ L", "ELI:", "( 1 )", "( 2 )"] {
                assert!(
                    !c.text.contains(pat),
                    "footnote contamination {:?} in {}",
                    pat,
                    c.code
                );
            }
        }
    }

    #[test]
    fn lookup_round_trips() {
        assert!(find("VI.1.1.1").is_some());
        assert!(find("VI.8.1").is_some());
        assert!(find("nonsense").is_none());
    }
}
