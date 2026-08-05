use crate::{
    Connection, Error, MatchRule, ReleaseNameReply, RequestNameFlags, RequestNameReply, Result,
    name::{validate_unique_name, validate_well_known_name},
};

const DBUS_DESTINATION: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

impl Connection {
    pub async fn request_name(
        &mut self,
        name: &str,
        flags: RequestNameFlags,
    ) -> Result<RequestNameReply> {
        self.ensure_bus()?;
        validate_well_known_name(name, "well-known bus name")?;
        let reply: u32 = self
            .call(
                Some(DBUS_DESTINATION),
                DBUS_PATH,
                Some(DBUS_INTERFACE),
                "RequestName",
                &(name, flags.bits()),
            )
            .await?;
        RequestNameReply::try_from(reply)
    }

    pub async fn release_name(&mut self, name: &str) -> Result<ReleaseNameReply> {
        self.ensure_bus()?;
        validate_well_known_name(name, "well-known bus name")?;
        let reply: u32 = self
            .call(
                Some(DBUS_DESTINATION),
                DBUS_PATH,
                Some(DBUS_INTERFACE),
                "ReleaseName",
                &(name,),
            )
            .await?;
        ReleaseNameReply::try_from(reply)
    }

    /// Installs a validated signal match on the bus.
    ///
    /// A well-known sender mutates `rule` with its validated current unique
    /// owner and installs an exact `NameOwnerChanged` match used by
    /// [`MatchRule::observe`] and [`crate::SignalStream`].
    pub async fn add_match(&mut self, rule: &mut MatchRule) -> Result<()> {
        self.ensure_bus()?;
        self.add_match_expression(rule.bus_expression()).await?;
        let Some(name) = rule.well_known_sender().map(str::to_owned) else {
            return Ok(());
        };
        if let Some(expression) = rule.owner_change_expression()
            && let Err(error) = self.add_match_expression(expression).await
        {
            let _ = self.remove_match_expression(rule.bus_expression()).await;
            return Err(error);
        }
        let owner = match self
            .call::<_, String>(
                Some(DBUS_DESTINATION),
                DBUS_PATH,
                Some(DBUS_INTERFACE),
                "GetNameOwner",
                &(name.as_str(),),
            )
            .await
        {
            Ok(owner) => Some(owner),
            Err(Error::Method { name, .. })
                if name == "org.freedesktop.DBus.Error.NameHasNoOwner" =>
            {
                None
            }
            Err(error) => {
                if let Some(expression) = rule.owner_change_expression() {
                    let _ = self.remove_match_expression(expression).await;
                }
                let _ = self.remove_match_expression(rule.bus_expression()).await;
                return Err(error);
            }
        };
        if let Some(owner) = &owner
            && let Err(error) = validate_unique_name(owner, "signal sender owner")
        {
            if let Some(expression) = rule.owner_change_expression() {
                let _ = self.remove_match_expression(expression).await;
            }
            let _ = self.remove_match_expression(rule.bus_expression()).await;
            return Err(error);
        }
        rule.set_owner(owner);
        Ok(())
    }

    async fn add_match_expression(&mut self, rule: &str) -> Result<()> {
        self.call(
            Some(DBUS_DESTINATION),
            DBUS_PATH,
            Some(DBUS_INTERFACE),
            "AddMatch",
            &(rule,),
        )
        .await
    }

    /// Removes a previously installed signal match from the bus.
    pub async fn remove_match(&mut self, rule: &MatchRule) -> Result<()> {
        self.ensure_bus()?;
        let primary = self.remove_match_expression(rule.bus_expression()).await;
        let owner_change = match rule.owner_change_expression() {
            Some(expression) => self.remove_match_expression(expression).await,
            None => Ok(()),
        };
        primary.and(owner_change)
    }

    async fn remove_match_expression(&mut self, rule: &str) -> Result<()> {
        self.call(
            Some(DBUS_DESTINATION),
            DBUS_PATH,
            Some(DBUS_INTERFACE),
            "RemoveMatch",
            &(rule,),
        )
        .await
    }
}
