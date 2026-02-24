use monochrome::{album::Album, artist::Artist, id::AlbumId, track::Track};

#[derive(Debug, Clone)]
pub enum TrackOrAlbum {
    Track(Track),
    Album(Album),
}

impl TrackOrAlbum {
    pub fn title(&self) -> &str {
        match self {
            TrackOrAlbum::Track(track) => &track.title,
            TrackOrAlbum::Album(album) => &album.title,
        }
    }

    pub fn artists(&self) -> &[Artist] {
        match self {
            TrackOrAlbum::Track(track) => &track.artists,
            TrackOrAlbum::Album(album) => &album.artists,
        }
    }

    pub fn artist(&self) -> &Artist {
        match self {
            TrackOrAlbum::Track(track) => &track.artist,
            TrackOrAlbum::Album(album) => &album.artist,
        }
    }

    pub fn cover(&self) -> uuid::Uuid {
        match self {
            TrackOrAlbum::Track(track) => track.album.cover,
            TrackOrAlbum::Album(album) => album.cover,
        }
    }

    pub fn release_date(&self) -> chrono::NaiveDate {
        match self {
            TrackOrAlbum::Track(track) => Default::default(), // tracks don't have release dates, so just return the default value
            TrackOrAlbum::Album(album) => album.release_date,
        }
    }

    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        TrackIteratorRef { i: 0, s: self }
    }

    pub fn into_tracks(self) -> impl Iterator<Item = Track> {
        TrackIteratorOwned { i: 0, s: self }
    }

    pub fn is_track(&self) -> bool {
        matches!(self, TrackOrAlbum::Track(_))
    }

    pub fn album_id(&self) -> AlbumId {
        match self {
            TrackOrAlbum::Track(track) => track.album.id,
            TrackOrAlbum::Album(album) => album.id,
        }
    }
}

pub struct TrackIteratorRef<'a> {
    i: usize,
    s: &'a TrackOrAlbum,
}

impl<'a> Iterator for TrackIteratorRef<'a> {
    type Item = &'a Track;

    fn next(&mut self) -> Option<Self::Item> {
        match self.s {
            TrackOrAlbum::Track(track) => {
                if self.i == 0 {
                    self.i += 1;
                    Some(track)
                } else {
                    None
                }
            }

            TrackOrAlbum::Album(album) => {
                if self.i < album.tracks.len() {
                    let track = &album.tracks[self.i];
                    self.i += 1;
                    Some(track)
                } else {
                    None
                }
            }
        }
    }
}

pub struct TrackIteratorOwned {
    i: usize,
    s: TrackOrAlbum,
}

impl Iterator for TrackIteratorOwned {
    type Item = Track;

    fn next(&mut self) -> Option<Self::Item> {
        match &self.s {
            TrackOrAlbum::Track(track) => {
                if self.i == 0 {
                    self.i += 1;
                    Some(track.clone())
                } else {
                    None
                }
            }

            TrackOrAlbum::Album(album) => {
                if self.i < album.tracks.len() {
                    let track = album.tracks[self.i].clone();
                    self.i += 1;
                    Some(track)
                } else {
                    None
                }
            }
        }
    }
}
