use zero_schema::{ErrorKind, FieldDescriptor, SchemaError, zero};

// Zero is deliberately not a logical value, making this scalar enum eligible for
// the zero-sentinel option protocol.
#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Mode {
    Enabled = 1,
    Disabled = 2,
}

#[zero]
#[derive(Debug, Eq, PartialEq)]
pub struct Profile {
    mode: Mode,
}

#[zero]
#[derive(Debug, Eq, PartialEq)]
pub struct Settings {
    prefix: u8,
    // The field itself—not an Option discriminant—owns this eight-byte
    // all-zero sentinel span. Its seven trailing bytes are field-local padding.
    #[zero(align = 8)]
    mode: Option<Mode>,
    profile: Option<Profile>,
    modes: Option<[Mode; 2]>,
    suffix: u8,
}

// Simulated producer-owned bytes. The producer independently reviewed this
// all-zero representation as `None` for every optional field.
#[repr(C)]
struct ProducerSettings {
    bytes: [u8; Settings::SCHEMA_SIZE],
}

impl ProducerSettings {
    const fn reviewed_all_none() -> Self {
        Self {
            bytes: [0; Settings::SCHEMA_SIZE],
        }
    }
}

fn field(name: &str) -> &'static FieldDescriptor {
    Settings::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .expect("declared Settings field")
}

fn main() {
    let producer = ProducerSettings::reviewed_all_none();
    let mut storage = zero_schema::make_schema_buffer!(Settings);
    let mode_field = field("mode");
    let mode_span = mode_field.offset()..mode_field.offset() + mode_field.size();
    let parent_padding = (0..Settings::SCHEMA_SIZE)
        .find(|byte| {
            !Settings::LAYOUT
                .fields()
                .iter()
                .any(|field| field.offset() <= *byte && *byte < field.offset() + field.size())
        })
        .expect("the aligned mode field leaves parent padding");

    // This option's full span includes field-local alignment padding, whereas
    // bytes not described by any field are parent padding and never participate.
    assert!(mode_field.is_optional());
    assert_eq!((mode_field.align(), mode_field.size()), (8, 8));
    assert_eq!(Mode::SCHEMA_SIZE, 1);
    assert!(mode_span.contains(&(mode_field.offset() + Mode::SCHEMA_SIZE)));

    // SchemaBuffer's initial zero fill is receiving-memory initialization, not
    // a general schema initializer. These particular producer bytes happen to
    // represent None; Rust copies them in before proving the schema access.
    storage.as_bytes_mut().copy_from_slice(&producer.bytes);
    storage.as_bytes_mut()[parent_padding] = 0xa5;
    {
        let received = Settings::access(storage.as_bytes())
            .expect("reviewed producer bytes with ignored parent padding are valid");
        assert_eq!(received.mode(), None);
        assert!(received.profile().is_none());
        assert!(received.modes().is_none());
        assert_eq!(
            received.copy_into(),
            Settings {
                prefix: 0,
                mode: None,
                profile: None,
                modes: None,
                suffix: 0,
            }
        );
    }

    {
        let mut settings = Settings::access_mut(storage.as_bytes_mut())
            .expect("the received optional sentinels are mutable");

        let mut mode = settings.mode_mut();
        assert!(mode.get().is_none());
        assert!(mode.get_mut().is_none());
        mode.set(Some(Mode::Enabled))
            .expect("a zero-invalid scalar initializes the optional field");
        assert_eq!(mode.get(), Some(Mode::Enabled));
        mode.get_mut()
            .expect("the scalar is present")
            .set(Mode::Disabled)
            .expect("the present scalar has a short mutable reborrow");
        assert_eq!(mode.get(), Some(Mode::Disabled));

        let mut profile = settings.profile_mut();
        assert!(profile.get().is_none());
        profile
            .set(Some(Profile {
                mode: Mode::Enabled,
            }))
            .expect("a complete Profile initializes the optional schema field");
        {
            let mut nested = profile
                .get_mut()
                .expect("the present Profile has a short mutable reborrow");
            nested
                .mode_mut()
                .set(Mode::Disabled)
                .expect("the nested scalar mutation is constrained to Profile");
        }
        assert_eq!(
            profile.get().map(|profile| profile.mode()),
            Some(Mode::Disabled)
        );

        let mut modes = settings.modes_mut();
        assert!(modes.get().is_none());
        modes
            .set(Some([Mode::Enabled, Mode::Disabled]))
            .expect("both array elements initialize before the option is present");
        assert_eq!(
            modes
                .get()
                .expect("the initialized array is present")
                .copy_into(),
            [Mode::Enabled, Mode::Disabled]
        );
    }

    // The local trailing padding is part of the scalar option's complete
    // sentinel span. It can be nonzero for a present value and set(None)
    // clears it, while unrelated parent padding is left untouched.
    storage.as_bytes_mut()[mode_field.offset() + Mode::SCHEMA_SIZE] = 0xc1;
    assert_eq!(
        Settings::access(storage.as_bytes())
            .expect("present scalar ignores its field-local padding after proof")
            .mode(),
        Some(Mode::Disabled)
    );
    {
        let mut settings = Settings::access_mut(storage.as_bytes_mut())
            .expect("the present scalar remains mutable");
        settings
            .mode_mut()
            .set(None)
            .expect("clearing zeroes the complete field-local sentinel span");
    }
    assert!(
        storage.as_bytes()[mode_span.clone()]
            .iter()
            .all(|byte| *byte == 0)
    );
    assert_eq!(storage.as_bytes()[parent_padding], 0xa5);

    let observed = Settings::access(storage.as_bytes())
        .expect("scalar clearing keeps the remaining borrowed values valid");
    assert_eq!(observed.mode(), None);
    assert_eq!(
        observed.profile().map(|profile| profile.mode()),
        Some(Mode::Disabled)
    );
    assert_eq!(
        observed
            .modes()
            .expect("the array remains present")
            .copy_into(),
        [Mode::Enabled, Mode::Disabled]
    );
    let copied = observed.copy_into();
    assert_eq!(
        copied.profile,
        Some(Profile {
            mode: Mode::Disabled
        })
    );
    assert_eq!(copied.modes, Some([Mode::Enabled, Mode::Disabled]));

    // Clear all direct writes before exercising absent-field patch promotion.
    {
        let mut settings = Settings::access_mut(storage.as_bytes_mut())
            .expect("the direct optional writes remain valid");
        settings
            .profile_mut()
            .set(None)
            .expect("schema clearing writes its complete sentinel span");
        settings
            .modes_mut()
            .set(None)
            .expect("array clearing writes its complete sentinel span");
    }

    // Outer None retains a field byte-for-byte, even when every option is absent.
    let mut before = [0; Settings::SCHEMA_SIZE];
    before.copy_from_slice(storage.as_bytes());
    Settings::access_mut(storage.as_bytes_mut())
        .expect("absent options are valid patch targets")
        .copy_from(&SettingsPatch {
            profile: None,
            ..Default::default()
        })
        .expect("a retain patch is a no-op");
    assert_eq!(storage.as_bytes(), &before);

    // Some(Some(_)) can promote absence only when the nested patch is complete;
    // failure is transactional across the complete destination buffer.
    let incomplete = SettingsPatch {
        profile: Some(Some(ProfilePatch { mode: None })),
        ..Default::default()
    };
    let error = Settings::access_mut(storage.as_bytes_mut())
        .expect("the retained bytes still describe absent optionals")
        .copy_from(&incomplete)
        .expect_err("an absent Profile cannot be initialized from a partial patch");
    assert_eq!(error.kind(), ErrorKind::IncompleteOptionalInitialization);
    assert_eq!(storage.as_bytes(), &before);

    let complete = SettingsPatch {
        mode: Some(Some(Mode::Disabled.into())),
        profile: Some(Some(ProfilePatch {
            mode: Some(Mode::Enabled.into()),
        })),
        modes: Some(Some([Mode::Enabled, Mode::Disabled])),
        ..Default::default()
    };
    Settings::access_mut(storage.as_bytes_mut())
        .expect("absent options are valid patch targets")
        .copy_from(&complete)
        .expect("complete scalar, schema, and array patches promote absence");

    // Retain does not replace a present value, and Some(None) is the distinct
    // third state that clears only the selected optional field.
    let mut before_retain = [0; Settings::SCHEMA_SIZE];
    before_retain.copy_from_slice(storage.as_bytes());
    Settings::access_mut(storage.as_bytes_mut())
        .expect("promoted options are valid patch targets")
        .copy_from(&SettingsPatch {
            profile: None,
            ..Default::default()
        })
        .expect("outer None retains the present Profile");
    assert_eq!(storage.as_bytes(), &before_retain);
    Settings::access_mut(storage.as_bytes_mut())
        .expect("retain leaves the options valid")
        .copy_from(&SettingsPatch {
            profile: Some(None),
            ..Default::default()
        })
        .expect("Some(None) clears the Profile sentinel span");
    assert!(
        storage.as_bytes()
            [field("profile").offset()..field("profile").offset() + field("profile").size()]
            .iter()
            .all(|byte| *byte == 0)
    );

    // A full logical value converts to a complete patch, including explicit
    // optional presence and absence states, then a fresh access borrows it.
    let full = SettingsPatch::from(Settings {
        prefix: 7,
        mode: Some(Mode::Enabled),
        profile: Some(Profile {
            mode: Mode::Disabled,
        }),
        modes: Some([Mode::Disabled, Mode::Enabled]),
        suffix: 9,
    });
    assert!(matches!(&full.mode, Some(Some(_))));
    assert!(matches!(&full.profile, Some(Some(_))));
    assert!(matches!(&full.modes, Some(Some(_))));
    Settings::access_mut(storage.as_bytes_mut())
        .expect("cleared Profile leaves a valid root")
        .copy_from(&full)
        .expect("From<Settings> supplies a complete patch");

    let refreshed = Settings::access(storage.as_bytes())
        .expect("a fresh access observes every final optional shape");
    assert_eq!(refreshed.prefix(), 7);
    assert_eq!(refreshed.mode(), Some(Mode::Enabled));
    assert_eq!(
        refreshed.profile().map(|profile| profile.mode()),
        Some(Mode::Disabled)
    );
    assert_eq!(
        refreshed
            .modes()
            .expect("final array is present")
            .copy_into(),
        [Mode::Disabled, Mode::Enabled]
    );
    assert_eq!(refreshed.suffix(), 9);
    assert_eq!(storage.as_bytes()[parent_padding], 0xa5);

    println!("optional scalar, schema, and array fields exercised transactionally");
}
