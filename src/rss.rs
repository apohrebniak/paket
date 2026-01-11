use crate::FeedItem;
use crate::FeedWriter;
use crate::WeeklyItem;
use httpdate::fmt_http_date;
use std::time::SystemTime;

pub struct RssWriter {
    buffer: String,
}

impl FeedWriter for RssWriter {
    const CONTENT_TYPE: &str = "application/rss+xml";

    fn new(title: &str, description: &str, link: &str, time: SystemTime) -> Self {
        let mut buffer = String::new();

        let date = fmt_http_date(time);

        buffer.push_str("<rss version=\"2.0\">");
        buffer.push_str("<channel>");

        buffer.push_str("<title>");
        buffer.push_str(title);
        buffer.push_str("</title>");

        buffer.push_str("<description>");
        buffer.push_str(description);
        buffer.push_str("</description>");

        buffer.push_str("<link>");
        buffer.push_str(link);
        buffer.push_str("</link>");

        buffer.push_str("<pubDate>");
        buffer.push_str(date.as_str());
        buffer.push_str("</pubDate>");

        buffer.push_str("<lastBuildDate>");
        buffer.push_str(date.as_str());
        buffer.push_str("</lastBuildDate>");

        buffer.push_str("<ttl>0</ttl>");

        Self { buffer }
    }

    fn write_weekly_items(&mut self, _: Vec<WeeklyItem>) { /* noop */
    }

    fn write_feed_items(&mut self, items: Vec<FeedItem>) {
        let buffer = &mut self.buffer;

        for item in items {
            buffer.push_str("<item>");

            buffer.push_str("<title>");
            buffer.push_str(item.title.as_str());
            buffer.push_str("</title>");

            buffer.push_str("<link>");
            buffer.push_str(item.link.as_str());
            buffer.push_str("</link>");

            buffer.push_str("<pubDate>");
            buffer.push_str(item.pub_date.as_str());
            buffer.push_str("</pubDate>");

            buffer.push_str("<guid>");
            buffer.push_str(item.guid.as_str());
            buffer.push_str("</guid>");

            buffer.push_str("</item>");
        }
    }

    fn finish(mut self) -> String {
        let buffer = &mut self.buffer;

        buffer.push_str("</channel>");
        buffer.push_str("</rss>");

        self.buffer
    }
}
