use {
    crate::{
        client::{Client, ClientError, ClientId},
        ifs::{
            ipc::{
                break_device_loops, destroy_device,
                zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
                zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
                zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1, DeviceData,
                OfferData, Role, SourceData, Vtable,
            },
            wl_seat::{WlSeat, WlSeatError, WlSeatGlobal},
        },
        leaks::Tracker,
        object::{Object, ObjectId},
        utils::buffd::{MsgParser, MsgParserError},
        wire::{
            zwp_primary_selection_device_v1::*, ZwpPrimarySelectionDeviceV1Id,
            ZwpPrimarySelectionOfferV1Id,
        },
    },
    std::rc::Rc,
    thiserror::Error,
    uapi::OwnedFd,
};

pub struct ZwpPrimarySelectionDeviceV1 {
    pub id: ZwpPrimarySelectionDeviceV1Id,
    pub manager: Rc<ZwpPrimarySelectionDeviceManagerV1>,
    seat: Rc<WlSeat>,
    data: DeviceData<Self>,
    pub tracker: Tracker<Self>,
}

impl ZwpPrimarySelectionDeviceV1 {
    pub fn new(
        id: ZwpPrimarySelectionDeviceV1Id,
        manager: &Rc<ZwpPrimarySelectionDeviceManagerV1>,
        seat: &Rc<WlSeat>,
    ) -> Self {
        Self {
            id,
            manager: manager.clone(),
            seat: seat.clone(),
            data: DeviceData::default(),
            tracker: Default::default(),
        }
    }

    pub fn send_data_offer(&self, offer: ZwpPrimarySelectionOfferV1Id) {
        self.manager.client.event(DataOffer {
            self_id: self.id,
            offer,
        })
    }

    pub fn send_selection(&self, id: ZwpPrimarySelectionOfferV1Id) {
        self.manager.client.event(Selection {
            self_id: self.id,
            id,
        })
    }

    fn set_selection(
        &self,
        parser: MsgParser<'_, '_>,
    ) -> Result<(), ZwpPrimarySelectionDeviceV1Error> {
        let req: SetSelection = self.manager.client.parse(self, parser)?;
        if !self.manager.client.valid_serial(req.serial) {
            log::warn!("Client tried to set_selection with an invalid serial");
            return Ok(());
        }
        if !self
            .seat
            .global
            .may_modify_primary_selection(&self.seat.client, req.serial)
        {
            log::warn!("Ignoring disallowed set_selection request");
            return Ok(());
        }
        let src = if req.source.is_none() {
            None
        } else {
            Some(self.manager.client.lookup(req.source)?)
        };
        self.seat
            .global
            .set_primary_selection(src, Some(req.serial))?;
        Ok(())
    }

    fn destroy(&self, parser: MsgParser<'_, '_>) -> Result<(), ZwpPrimarySelectionDeviceV1Error> {
        let _req: Destroy = self.manager.client.parse(self, parser)?;
        destroy_device::<Self>(self);
        self.seat.remove_primary_selection_device(self);
        self.manager.client.remove_obj(self)?;
        Ok(())
    }
}

impl Vtable for ZwpPrimarySelectionDeviceV1 {
    type DeviceId = ZwpPrimarySelectionDeviceV1Id;
    type OfferId = ZwpPrimarySelectionOfferV1Id;
    type Device = ZwpPrimarySelectionDeviceV1;
    type Source = ZwpPrimarySelectionSourceV1;
    type Offer = ZwpPrimarySelectionOfferV1;

    fn device_id(dd: &Self::Device) -> Self::DeviceId {
        dd.id
    }

    fn get_device_data(dd: &Self::Device) -> &DeviceData<Self> {
        &dd.data
    }

    fn get_offer_data(offer: &Self::Offer) -> &OfferData<Self> {
        &offer.offer_data
    }

    fn get_source_data(src: &Self::Source) -> &SourceData<Self> {
        &src.data
    }

    fn for_each_device<C>(seat: &WlSeatGlobal, client: ClientId, f: C)
    where
        C: FnMut(&Rc<Self::Device>),
    {
        seat.for_each_primary_selection_device(0, client, f)
    }

    fn create_offer(
        client: &Rc<Client>,
        _device: &Rc<ZwpPrimarySelectionDeviceV1>,
        offer_data: OfferData<Self>,
        id: ObjectId,
    ) -> Rc<Self::Offer> {
        let rc = Rc::new(ZwpPrimarySelectionOfferV1 {
            id: id.into(),
            client: client.clone(),
            offer_data,
            tracker: Default::default(),
        });
        track!(client, rc);
        rc
    }

    fn send_selection(dd: &Self::Device, offer: Self::OfferId) {
        dd.send_selection(offer);
    }

    fn send_cancelled(source: &Self::Source) {
        source.send_cancelled();
    }

    fn get_offer_id(offer: &Self::Offer) -> Self::OfferId {
        offer.id
    }

    fn send_offer(dd: &Self::Device, offer: &Self::Offer) {
        dd.send_data_offer(offer.id);
    }

    fn send_mime_type(offer: &Self::Offer, mime_type: &str) {
        offer.send_offer(mime_type);
    }

    fn unset(seat: &Rc<WlSeatGlobal>, _role: Role) {
        seat.unset_primary_selection();
    }

    fn send_send(src: &Self::Source, mime_type: &str, fd: Rc<OwnedFd>) {
        src.send_send(mime_type, fd);
    }
}

object_base! {
    ZwpPrimarySelectionDeviceV1;

    SET_SELECTION => set_selection,
    DESTROY => destroy,
}

impl Object for ZwpPrimarySelectionDeviceV1 {
    fn num_requests(&self) -> u32 {
        DESTROY + 1
    }

    fn break_loops(&self) {
        break_device_loops::<Self>(self);
        self.seat.remove_primary_selection_device(self);
    }
}

simple_add_obj!(ZwpPrimarySelectionDeviceV1);

#[derive(Debug, Error)]
pub enum ZwpPrimarySelectionDeviceV1Error {
    #[error(transparent)]
    ClientError(Box<ClientError>),
    #[error("Parsing failed")]
    MsgParserError(#[source] Box<MsgParserError>),
    #[error(transparent)]
    WlSeatError(Box<WlSeatError>),
}
efrom!(ZwpPrimarySelectionDeviceV1Error, ClientError);
efrom!(ZwpPrimarySelectionDeviceV1Error, MsgParserError);
efrom!(ZwpPrimarySelectionDeviceV1Error, WlSeatError);
