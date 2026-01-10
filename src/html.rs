use crate::FeedItem;
use crate::FeedWriter;
use httpdate::fmt_http_date;
use std::time::SystemTime;

const STYLE: &str = include_str!("../style.css");

pub struct HtmlWriter {
    buffer: String,
}

impl FeedWriter for HtmlWriter {
    const CONTENT_TYPE: &str = "text/html";

    fn new(title: &str, description: &str, link: &str, date: SystemTime) -> Self {
        let mut buffer = String::new();

        let date = fmt_http_date(date);

        buffer.push_str(
            r#"
            <!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>
        "#,
        );

        buffer.push_str(title);

        buffer.push_str("</title><style>");

        buffer.push_str(STYLE);

        buffer.push_str("</style></head><body>");

        buffer.push_str("<h1>");
        buffer.push_str(title);
        buffer.push_str("</h1>");

        buffer.push_str("<h3>");
        buffer.push_str(description);
        buffer.push_str("</h3>");

        buffer.push_str("<div class=\"feed-info\">");

        buffer.push_str("<p>Feed: <a href=\"");
        buffer.push_str(link);
        buffer.push_str("\">");
        buffer.push_str(link);
        buffer.push_str("</a></p>");

        buffer.push_str("<p>Last Updated: ");
        buffer.push_str(&date);
        buffer.push_str("</p>");

        buffer.push_str("</div>");

        buffer.push_str(
            r#"
            <div class="month-labels">
                <span>Jan</span>
                <span>Mar</span>
                <span>Jun</span>
                <span>Sep</span>
                <span>Dec</span>
            </div>
            "#
        );

        buffer.push_str(
            r#"
            <div class="calendar" style="--max-articles: 20;">
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 12;" title="12 articles"></div>
                <div class="week-square" style="--articles: 8;" title="8 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
                <div class="week-square" style="--articles: 20;" title="20 articles"></div>
                <div class="week-square" style="--articles: 6;" title="6 articles"></div>
                <div class="week-square" style="--articles: 14;" title="14 articles"></div>
                <div class="week-square" style="--articles: 10;" title="10 articles"></div>
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 8;" title="8 articles"></div>
                <div class="week-square" style="--articles: 4;" title="4 articles"></div>
                <div class="week-square" style="--articles: 12;" title="12 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
                <div class="week-square" style="--articles: 20;" title="20 articles"></div>
                <div class="week-square" style="--articles: 6;" title="6 articles"></div>
                <div class="week-square" style="--articles: 14;" title="14 articles"></div>
                <div class="week-square" style="--articles: 10;" title="10 articles"></div>
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 8;" title="8 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
                <div class="week-square" style="--articles: 4;" title="4 articles"></div>
                <div class="week-square" style="--articles: 12;" title="12 articles"></div>
                <div class="week-square" style="--articles: 20;" title="20 articles"></div>
                <div class="week-square" style="--articles: 10;" title="10 articles"></div>
                <div class="week-square" style="--articles: 14;" title="14 articles"></div>
                <div class="week-square" style="--articles: 6;" title="6 articles"></div>
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 8;" title="8 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
                <div class="week-square" style="--articles: 12;" title="12 articles"></div>
                <div class="week-square" style="--articles: 20;" title="20 articles"></div>
                <div class="week-square" style="--articles: 6;" title="6 articles"></div>
                <div class="week-square" style="--articles: 14;" title="14 articles"></div>
                <div class="week-square" style="--articles: 10;" title="10 articles"></div>
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 8;" title="8 articles"></div>
                <div class="week-square" style="--articles: 4;" title="4 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
                <div class="week-square" style="--articles: 12;" title="12 articles"></div>
                <div class="week-square" style="--articles: 20;" title="20 articles"></div>
                <div class="week-square" style="--articles: 10;" title="10 articles"></div>
                <div class="week-square" style="--articles: 14;" title="14 articles"></div>
                <div class="week-square" style="--articles: 6;" title="6 articles"></div>
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 8;" title="8 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
                <div class="week-square" style="--articles: 12;" title="12 articles"></div>
                <div class="week-square" style="--articles: 20;" title="20 articles"></div>
                <div class="week-square" style="--articles: 6;" title="6 articles"></div>
                <div class="week-square" style="--articles: 14;" title="14 articles"></div>
                <div class="week-square" style="--articles: 10;" title="10 articles"></div>
                <div class="week-square" style="--articles: 18;" title="18 articles"></div>
                <div class="week-square" style="--articles: 16;" title="16 articles"></div>
            </div>
            "#
        );

        buffer.push_str("<ul class=\"feed-items\">");

        Self { buffer }
    }

    fn write_items(&mut self, items: impl Iterator<Item = FeedItem>) {
        let buffer = &mut self.buffer;

        for item in items {
            buffer.push_str("<li><article class=\"feed-item\"><h2><a href=\"");
            buffer.push_str(&item.link);
            buffer.push_str("\">");
            buffer.push_str(&item.title);
            buffer.push_str("</a></h2><div class=\"published-date\"> Published: ");
            buffer.push_str(&item.pub_date);
            buffer.push_str("</div><form method=\"POST\" action=\"/delete\" style=\"display: inline;\"><input type=\"hidden\" name=\"guid\" value=\"");
            buffer.push_str(&item.guid);
            buffer.push_str("\"><button type=\"submit\" class=\"delete-btn\">Delete</button></form></article></li>");
        }
    }

    fn finish(self) -> String {
        let mut buffer = self.buffer;

        buffer.push_str("</ul></body></html>");

        buffer
    }
}
