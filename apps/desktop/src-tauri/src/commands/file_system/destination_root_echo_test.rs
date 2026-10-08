//! `destination_root_echo`: the transfer dialog's warning when a typed path
//! repeats the place's own root folder (#164). The rule itself is
//! `cmdr_fs::volume::root_echo`; this pins that the command reads the right
//! volume's root and never answers for one it can't find.

use std::sync::Arc;

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, Volume};

use super::{DestinationRootEcho, destination_root_echo};

#[tokio::test]
async fn a_server_path_typed_on_a_server_place_names_both_readings() {
    let id = format!("root-echo-{}", uuid::Uuid::new_v4());
    let volume = InMemoryVolume::new("Server").with_root("sftp://ada@nas:22/srv/data");
    get_volume_manager().register(&id, Arc::new(volume) as Arc<dyn Volume>);

    let echo = destination_root_echo(id.clone(), "/srv/data/photos".to_string()).await;
    assert_eq!(
        echo,
        Some(DestinationRootEcho {
            root_folder: "/srv/data".to_string(),
            resolved: "/srv/data/srv/data/photos".to_string(),
            stripped: "/photos".to_string(),
        })
    );
    assert_eq!(destination_root_echo(id.clone(), "/photos".to_string()).await, None);
    get_volume_manager().unregister(&id);
}

#[tokio::test]
async fn an_unregistered_volume_says_nothing() {
    assert_eq!(
        destination_root_echo("nobody-here".to_string(), "/srv/data/photos".to_string()).await,
        None
    );
}
