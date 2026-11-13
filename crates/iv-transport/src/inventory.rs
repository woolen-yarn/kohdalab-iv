//! Keep an instrument's transport connection independent of its measurement role.
use super::*;

pub(super) struct RegisteredConnection {
    pub(super) endpoint: Endpoint,
    pub(super) kind: ResourceKind,
    pub(super) model: String,
    identity: String,
    role: Role,
    pub(super) recovery_pending: bool,
    pub(super) meter_discard: bool,
}

/// Identity and resource remain available even while an instrument has no
/// measurement role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredDevice {
    pub id: u64,
    pub source: bool,
    pub model: String,
    pub resource: String,
    pub identity: String,
    pub assigned_role: Option<Role>,
}

impl DevicePair {
    /// Transfer parked devices to the idle-control owner for the duration of
    /// a sweep. Shared helpers still serialize requests on the same mutex.
    pub fn take_parked_for_sweep(&mut self) -> Self {
        let mut parked = Self::new(&self.paths);
        parked.registered = std::mem::take(&mut self.registered);
        parked.usable = self.usable;
        parked.sweep_sessions = ROLES
            .into_iter()
            .filter_map(|role| Some((role, self.endpoint_option(role)?.session.clone())))
            .collect();
        parked
    }

    /// Rejoin after the parked owner has stopped. If sweep cleanup replaced a
    /// shared helper, revalidate parked instrument identities on next access.
    pub fn restore_parked_after_sweep(&mut self, mut parked: Self) {
        for (role, old) in &parked.sweep_sessions {
            if let Some(endpoint) = self.endpoint_option(*role) {
                for entry in parked.registered.values_mut() {
                    if Arc::ptr_eq(&entry.endpoint.session, old)
                        && !Arc::ptr_eq(&endpoint.session, old)
                    {
                        entry.endpoint.session = endpoint.session.clone();
                        entry.recovery_pending = true;
                    }
                }
            }
        }
        self.registered.append(&mut parked.registered);
    }

    /// Connect an inventory entry without occupying a measurement role.
    /// Identity is checked before the connection is added to the inventory.
    pub fn connect_registered(
        &mut self,
        id: u64,
        role: Role,
        resource: &str,
        model: &str,
    ) -> Result<(), String> {
        if self.registered.contains_key(&id) || self.registered_roles.contains(&Some(id)) {
            return Err(format!("Instrument {id} is already connected."));
        }
        if !instruments::valid_model(role, model) {
            return Err(format!("Unsupported model: {model}"));
        }
        let kind = self.connection_kind(resource, model)?;
        let (endpoint, identity) = self.connect_endpoint(role, resource, model, &kind)?;
        self.registered.insert(
            id,
            RegisteredConnection {
                endpoint,
                kind,
                model: model.to_owned(),
                identity,
                role,
                recovery_pending: false,
                meter_discard: drivers::is_adc_meter(model),
            },
        );
        Ok(())
    }

    pub fn registered_devices(&self) -> Vec<RegisteredDevice> {
        let mut devices: Vec<_> = self
            .registered
            .iter()
            .map(|(&id, connection)| RegisteredDevice {
                id,
                source: is_source_role(connection.role),
                model: connection.model.clone(),
                resource: connection.endpoint.resource.clone(),
                identity: connection.identity.clone(),
                assigned_role: None,
            })
            .collect();
        for role in ROLES {
            if let Some(id) = self.registered_roles[role as usize] {
                devices.push(RegisteredDevice {
                    id,
                    source: is_source_role(role),
                    model: self.model(role).to_owned(),
                    resource: self
                        .resource(role)
                        .expect("registered role has an endpoint")
                        .to_owned(),
                    identity: self.identity(role).to_owned(),
                    assigned_role: Some(role),
                });
            }
        }
        devices.sort_unstable_by_key(|device| device.id);
        devices
    }

    /// Validate the complete selection before moving any connection.
    /// Unmentioned roles retain their previous bindings. Legacy connections
    /// must be disconnected explicitly and cannot be overwritten.
    pub fn select_registered(&mut self, selection: &[(Role, Option<u64>)]) -> Result<(), String> {
        let mut desired = self.registered_roles;
        let mut specified = [false; ROLES.len()];
        for &(role, id) in selection {
            let index = role as usize;
            if std::mem::replace(&mut specified[index], true) {
                return Err(format!("{} was assigned twice.", role_name(role)));
            }
            if self.is_connected(role) && desired[index].is_none() {
                return Err(format!(
                    "Disconnect {} before changing its assignment.",
                    role_name(role)
                ));
            }
            desired[index] = id;
        }
        let mut assigned = std::collections::BTreeSet::new();
        for role in ROLES {
            let Some(id) = desired[role as usize] else {
                continue;
            };
            if !assigned.insert(id) {
                return Err(format!("Instrument {id} cannot serve two roles."));
            }
            let source = if let Some(connection) = self.registered.get(&id) {
                is_source_role(connection.role)
            } else {
                let existing = ROLES
                    .into_iter()
                    .find(|role| self.registered_roles[*role as usize] == Some(id))
                    .ok_or_else(|| format!("Instrument {id} is not connected."))?;
                is_source_role(existing)
            };
            if source != is_source_role(role) {
                return Err(format!(
                    "Instrument {id} cannot be used as {}.",
                    role_name(role)
                ));
            }
        }
        let changes: Vec<_> = ROLES
            .into_iter()
            .filter(|role| desired[*role as usize] != self.registered_roles[*role as usize])
            .collect();
        for role in ROLES {
            if (specified[role as usize] || changes.contains(&role))
                && self.registered_roles[role as usize].is_some()
            {
                self.verify_unbind(role)?;
            }
        }
        for &role in &changes {
            if let Some(connection) = desired[role as usize].and_then(|id| self.registered.get(&id))
                && is_source_role(role)
            {
                verify_off(&connection.endpoint, &connection.model)?;
            }
        }
        for &role in &changes {
            if let Some(id) = self.registered_roles[role as usize] {
                self.park_role(role, id);
            }
        }
        for role in changes {
            if let Some(id) = desired[role as usize] {
                self.assign_registered(id, role);
            }
        }
        Ok(())
    }

    fn assign_registered(&mut self, id: u64, role: Role) {
        let connection = self.registered.remove(&id).expect("connection checked");
        self.install_role(
            role,
            &connection.model,
            connection.kind,
            connection.endpoint,
            connection.identity,
        );
        self.registered_roles[role as usize] = Some(id);
        self.recovery_pending[role as usize] = connection.recovery_pending;
        if !is_source_role(role) {
            *self.meter_discard_mut(role) = connection.meter_discard;
        }
    }

    fn verify_unbind(&mut self, role: Role) -> Result<(), String> {
        if is_source_role(role) {
            self.validate_recovery(role)?;
            verify_off(self.endpoint(role)?, self.model(role))?;
        }
        Ok(())
    }

    fn park_role(&mut self, role: Role, id: u64) {
        let connection = RegisteredConnection {
            endpoint: self
                .endpoint(role)
                .expect("registered role has an endpoint")
                .clone(),
            kind: self
                .kind_option(role)
                .expect("connected endpoint has a kind")
                .clone(),
            model: self.model(role).to_owned(),
            identity: self.identity(role).to_owned(),
            role,
            recovery_pending: self.recovery_pending[role as usize],
            meter_discard: !is_source_role(role) && *self.meter_discard_mut(role),
        };
        self.clear_role(role);
        self.registered.insert(id, connection);
    }

    /// Query an unassigned instrument through its existing connection and
    /// dialect. Assigned instruments are queried through their role owner.
    pub fn query_unassigned(&mut self, id: u64, command: &str) -> Result<String, String> {
        self.ensure_usable()?;
        let connection = self
            .registered
            .get_mut(&id)
            .ok_or_else(|| format!("Instrument {id} is not available for independent readback."))?;
        connection.verify_recovered_identity(id)?;
        let mut io = EndpointIo {
            endpoint: &connection.endpoint,
        };
        drivers::query(
            &mut io,
            connection.role,
            &connection.model,
            resource_is_usb(&connection.kind),
            &mut connection.meter_discard,
            command,
        )
    }

    /// Apply measurement settings to an unassigned meter. Sources require a
    /// verified DC controller before any setting or output writes.
    pub fn configure_unassigned_meter(&mut self, id: u64, command: &str) -> Result<(), String> {
        self.write_unassigned(id, false, command)
    }

    /// Write through an unassigned source's driver. Output control must use
    /// the measurement engine's verified DC sequence.
    pub fn write_unassigned_source(&mut self, id: u64, command: &str) -> Result<(), String> {
        self.write_unassigned(id, true, command)
    }

    fn write_unassigned(&mut self, id: u64, source: bool, command: &str) -> Result<(), String> {
        self.ensure_usable()?;
        let connection = self
            .registered
            .get_mut(&id)
            .filter(|entry| is_source_role(entry.role) == source)
            .ok_or_else(|| {
                format!("Instrument {id} is not available for independent configuration.")
            })?;
        connection.verify_recovered_identity(id)?;
        for command in drivers::write_commands(
            connection.role,
            &connection.model,
            resource_is_usb(&connection.kind),
            command,
        )? {
            connection
                .endpoint
                .request("WRITE", &drivers::terminated(&command))?;
        }
        if source {
            self.mark_adc_discards(Role::Source, command);
        }
        Ok(())
    }

    /// Request and verify OFF without reassigning the source. Recovery keeps
    /// the shared helper lease and revalidates identity before sending OFF.
    pub fn force_unassigned_output_off(&mut self, id: u64) -> Result<OutputOffReport, String> {
        if self
            .registered
            .get(&id)
            .is_none_or(|entry| !is_source_role(entry.role))
        {
            return Err(format!("Unassigned source {id} is unavailable."));
        }
        let deadline = Instant::now() + Duration::from_secs(28);
        let mut errors = match self.unassigned_off_once(id, deadline) {
            Ok(report) => return Ok(report),
            Err(error) => vec![error],
        };
        if self
            .sweep_sessions
            .iter()
            .any(|(_, session)| Arc::ptr_eq(session, &self.registered[&id].endpoint.session))
        {
            return Err(format!(
                "Could not confirm output OFF while the sweep owns the shared helper. {}",
                errors.join(" ")
            ));
        }
        for _ in 0..2 {
            if Instant::now() >= deadline {
                break;
            }
            let entry = &self.registered[&id];
            let session = entry.endpoint.session.clone();
            let kind = entry.kind.clone();
            match self
                .replace_helper(&session, &kind, deadline)
                .and_then(|()| self.unassigned_off_once(id, deadline))
            {
                Ok(report) => return Ok(report),
                Err(error) => errors.push(error),
            }
        }
        Err(format!(
            "Could not confirm output OFF after recovery. Verify the instrument at its front panel. {}",
            errors.join(" ")
        ))
    }

    fn unassigned_off_once(
        &mut self,
        id: u64,
        deadline: Instant,
    ) -> Result<OutputOffReport, String> {
        let entry = self
            .registered
            .get_mut(&id)
            .expect("registered source checked");
        entry.verify_recovered_identity_before(
            id,
            deadline.min(Instant::now() + Duration::from_secs(5)),
        )?;
        entry
            .endpoint
            .request_before("WRITE", source_off_command(&entry.model), deadline)?;
        Self::verify_off_and_zero(&entry.endpoint, &entry.model, deadline)
    }

    /// Return a registered instrument to local control. Keep ownership if
    /// output OFF or local control cannot be confirmed.
    pub fn disconnect_registered(&mut self, id: u64) -> Result<(), String> {
        if let Some(role) = ROLES
            .into_iter()
            .find(|role| self.registered_roles[*role as usize] == Some(id))
        {
            if is_source_role(role) && !self.force_output_off_role(role)?.output_off_verified {
                return Err(format!(
                    "Output OFF could not be verified for instrument {id}."
                ));
            }
            return self.disconnect(role);
        }
        let Some(connection) = self.registered.get_mut(&id) else {
            return Ok(());
        };
        connection.verify_recovered_identity(id)?;
        let connection = &self.registered[&id];
        if is_source_role(connection.role) {
            verify_off(&connection.endpoint, &connection.model)?;
        }
        let shared = self
            .attached_connections()
            .filter(|(_, endpoint)| Arc::ptr_eq(&endpoint.session, &connection.endpoint.session))
            .nth(1)
            .is_some();
        let result = Self::release_endpoint(
            &connection.endpoint,
            &connection.kind,
            &connection.model,
            shared,
        )?;
        self.registered.remove(&id);
        result
    }
}

impl RegisteredConnection {
    fn verify_recovered_identity(&mut self, id: u64) -> Result<(), String> {
        self.verify_recovered_identity_before(id, Instant::now() + Duration::from_secs(5))
    }

    fn verify_recovered_identity_before(
        &mut self,
        id: u64,
        deadline: Instant,
    ) -> Result<(), String> {
        if self.recovery_pending {
            let identity =
                identify_endpoint_before(&self.endpoint, self.role, &self.model, deadline)?;
            instruments::verify_model(self.role, &self.model, &identity)?;
            if normalize_identity(&identity) != normalize_identity(&self.identity) {
                return Err(format!(
                    "Instrument {id} identity changed after transport recovery."
                ));
            }
            self.recovery_pending = false;
        }
        Ok(())
    }
}

fn verify_off(endpoint: &Endpoint, model: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let off = if drivers::is_legacy_source(model) {
        !instruments::legacy_output(&endpoint.request_before("QUERY", "OC;E\n", deadline)?)?
    } else {
        let reply = endpoint.request_before("QUERY", ":OUTP?\n", deadline)?;
        matches!(
            reply.trim().to_ascii_uppercase().as_str(),
            "0" | "+0" | "OFF"
        )
    };
    if off {
        Ok(())
    } else {
        Err(format!(
            "Output OFF could not be verified for {}. Use the front panel to verify it before changing the assignment.",
            endpoint.resource
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn registered_sources_keep_one_gpib_session_and_cannot_be_unbound_while_on() {
        let _guard = crate::tests::GPIB_TEST_LOCK.lock().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "kohdalab-iv-registered-gpib-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let helper = directory.join("helper");
        let log = directory.join("commands");
        let enabled = directory.join("enabled");
        let script = format!(
            r#"#!/bin/sh
printf 'START\n' >> '{}'
printf 'KIV\tOK\t\n'
while read op address timeout payload; do
  printf '%s %s %s\n' "$op" "$address" "$payload" >> '{}'
  case "$op:$payload" in
    QUERY:2a49444e3f0a) reply=594f4b4f474157412c47533231302c4d4f434b2c312e3030 ;;
    QUERY:3a4f5554503f0a)
      if [ "$address" = 3 ] && [ -f '{}' ]; then reply=31; else reply=30; fi ;;
    *) reply='' ;;
  esac
  printf 'KIV\tOK\t%s\n' "$reply"
  if [ "$op" = EXIT ]; then break; fi
done
"#,
            log.display(),
            log.display(),
            enabled.display()
        );
        fs::write(&helper, script).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let mut devices = DevicePair::new(&HelperPaths {
            gpib: helper.clone(),
            usbtmc: helper,
            setup: None,
        });

        devices
            .connect_registered(20, Role::Source, "GPIB0::3::INSTR", "YOKOGAWA_GS210")
            .unwrap();
        devices
            .connect_registered(21, Role::Source, "GPIB0::4::INSTR", "YOKOGAWA_GS210")
            .unwrap();
        assert!(
            devices
                .connect_registered(22, Role::Source, "GPIB0::3::INSTR", "YOKOGAWA_GS210")
                .is_err()
        );
        assert!(
            devices
                .select_registered(&[(Role::Meter, Some(20))])
                .is_err()
        );
        assert!(
            devices
                .select_registered(&[(Role::Source, Some(999))])
                .is_err()
        );
        let initial = [(Role::Source, Some(21)), (Role::Source2, Some(20))];
        devices.select_registered(&initial).unwrap();
        assert!(
            devices
                .select_registered(&[(Role::Source, Some(20))])
                .is_err()
        );
        assert!(
            devices
                .select_registered(&[(Role::Source, Some(21)), (Role::Source, None)])
                .is_err()
        );

        let before = devices.registered_devices();
        fs::write(&enabled, "").unwrap();
        let swapped = [(Role::Source, Some(20)), (Role::Source2, Some(21))];
        assert!(devices.select_registered(&swapped).is_err());
        assert_eq!(
            devices.registered_devices(),
            before,
            "a failed source OFF check cannot partially swap roles"
        );
        fs::remove_file(&enabled).unwrap();
        devices.select_registered(&swapped).unwrap();
        assert_eq!(devices.resource(Role::Source), Some("GPIB0::3::INSTR"));
        assert_eq!(devices.resource(Role::Source2), Some("GPIB0::4::INSTR"));

        devices.select_registered(&[(Role::Source, None)]).unwrap();
        let parked = devices.registered_devices();
        assert_eq!(parked[0].id, 20);
        assert_eq!(parked[0].assigned_role, None);
        assert_eq!(parked[0].resource, "GPIB0::3::INSTR");
        assert_eq!(parked[1].id, 21);
        assert_eq!(parked[1].assigned_role, Some(Role::Source2));
        assert_eq!(devices.query_unassigned(20, ":OUTP?").unwrap().trim(), "0");
        assert!(devices.query_unassigned(21, ":OUTP?").is_err());
        assert_eq!(devices.registered_devices(), parked);
        devices.disconnect_registered(20).unwrap();
        assert_eq!(devices.query(Role::Source2, ":OUTP?").unwrap().trim(), "0");
        devices.close().unwrap();

        let log = fs::read_to_string(log).unwrap();
        assert_eq!(log.lines().filter(|line| *line == "START").count(), 1);
        assert_eq!(
            log.lines().filter(|line| line.starts_with("EXIT ")).count(),
            1
        );
        assert!(log.contains("LOCAL 3 ") && log.contains("LOCAL 4 "));
        fs::remove_dir_all(directory).unwrap();
    }
}
