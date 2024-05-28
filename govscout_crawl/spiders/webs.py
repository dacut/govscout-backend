import re
from scrapy import FormRequest, Request, Selector, Spider
from scrapy.loader import ItemLoader
from scrapy.http import Response
from random import randint
from typing import Dict, List, Optional
from urllib.parse import urljoin
from ..items import WebsOpportunityItem, WebsVendorItem, WebsDocumentItem

# FIXME: remove these
USERNAME = "dacut@ionosphere.io"
PASSWORD = "jc5yhs9$Y@bgEjXi"

# Regex to extract the target and argument from a __doPostBack call
DOPOSTBACK_RE = re.compile(r"__doPostBack\('([^']*)','([^']*)'\)")


class WebsSpider(Spider):
    """Spider for logging in to the Washington State Electronic Business Solution (WEBS) website."""

    name = "webs"
    start_urls = ["https://pr-webs-vendor.des.wa.gov/LoginPage.aspx"]

    def parse(self, response: Response):
        return FormRequest.from_response(
            response,
            formdata={
                "txtEmail": USERNAME,
                "txtPassword": PASSWORD,
                "Image1.x": "33",
                "Image1.y": "1",
            },
            callback=self.after_login,
        )

    def after_login(self, response: Response):
        if response.url.endswith("LoginPage.aspx"):
            self.logger.error("Failed to log in; final URL: %s", response.url)
            return

        self.logger.info("Logged in successfully; final URL: %s", response.url)

        # Visit the opportunity search page
        url = urljoin(response.url, "/Search_Bid.aspx")

        yield Request(url=url, callback=self.submit_opportunity_search)

    def submit_opportunity_search(self, response: Response):
        # Submit the opportunity search form
        req = FormRequest.from_response(
            response,
            formdata={
                "radCommCodes": "1",  # All commodity codes
                "radCounties": "1",  # All counties
                "ddlOrgName": "0",
                "textBoxBidCustRefNum": "",
                "Image1.x": "30",
                "Image1.y": "3",
            },
            callback=self.first_opportunity_listing_page,
        )
        body = req.body.replace(b"Image1=&", b"")
        req = req.replace(body=body)
        yield req

    def first_opportunity_listing_page(self, response: Response):
        if not response.url.endswith("/Search_Bid_Result.aspx"):
            self.logger.error("Failed to retrieve opportunity search page")
            return
        
        # Send the details from this page
        listing_page = 1
        yield from self.parse_opportunity_listing_page(response, listing_page)

        # Links the next listing pages are in tr tags with class Grid3Pager
        next_pages = response.xpath("//tr[@class='Grid3Pager']//a/@href").getall()

        # Find the next pages in this listing
        for jsurl in next_pages:
            # Convert the href from javascript:__doPostBack('TARGET','ARGUMENT')
            # to a form submission.
            m = DOPOSTBACK_RE.search(jsurl)
            if not m:
                self.logger.error("Failed to parse __doPostBack URL: %s", jsurl)
                continue

            listing_page += 1

            target, argument = m.groups()
            yield FormRequest(
                url=response.url,
                formdata={
                    "__EVENTTARGET": target,
                    "__EVENTARGUMENT": argument,
                },
                callback=self.parse_opportunity_listing_page,
                cb_kwargs={"listing_page": listing_page},
            )

    def parse_opportunity_listing_page(self, response: Response, listing_page: int):
        self.logger.info("Opportunity listing page %d: %s", listing_page, response.url)

        # Each detail page is in a tr with class Grid3File1 or Grid3File2
        for url in response.xpath(
            "//tr[@class='Grid3File1']//a[@class='ctext-hyperlink']/@href | //tr[@class='Grid3File2']//a[@class='ctext-hyperlink']/@href"
        ).getall():
            url = urljoin(response.url, url)
            yield Request(
                url=url, callback=self.parse_opportunity_detail_page_without_vendors,
                cb_kwargs={"listing_page": listing_page},
            )

    def parse_opportunity_detail_page_without_vendors(self, response: Response, listing_page: int):
        self.logger.info("Opportunity detail page w/o vendors (listing page %d): %s", listing_page, response.url)

        # Go ahead and get the essentials about the opportunity.
        item = WebsOpportunityItem()
        system_id = response.xpath("//span[@id='txtSystemIdentifier']/text()").get()
        item["system_id"] = system_id
        item["customer_ref_num"] = response.xpath(
            "//span[@id='txtReferenceNumber']/text()"
        ).get()
        item["org_name"] = response.xpath("//span[@id='txtOrgName']/text()").get()
        item["title"] = response.xpath("//span[@id='txtTitle']/text()").get()
        item["description"] = response.xpath(
            "//span[@id='txtDescription']/text()"
        ).get()
        item["date_posted"] = response.xpath("//span[@id='txtActiveDate']/text()").get()
        item["date_closed"] = response.xpath(
            "//span[@id='txtInactiveDate']/text()"
        ).get()
        item["estimated_value"] = response.xpath(
            "//span[@id='txtEstimatedValue']/text()"
        ).get()
        item["contact_name"] = response.xpath(
            "//span[@id='txtContactName']/text()"
        ).get()
        item["contact_phone"] = response.xpath(
            "//span[@id='txtContactPhone']/text()"
        ).get()
        item["contact_email"] = response.xpath("//span[@id='txtEmail']/text()").get()
        # Commodity codes has each line broken with a <br> tag. Retrieving the text removes the
        # <br> tags; we then need to remove surrounding whitespace.
        item["commodity_codes"] = [
            code.strip()
            for code in response.xpath("//span[@id='labelCommCodes']/text()").getall()
        ]

        # Counties is just a text string with commas separating the counties.
        item["counties"] = [
            county.strip()
            for county in response.xpath("//span[@id='labelCounties']/text()")
            .get()
            .split(",")
        ]
        # yield item

        # Extract each document from the page.
        for doc_html in response.xpath(
            "//table[@id='dataGridBidDocuments']//tr[@class='GridFile1']//td//a"
        ).getall():
            a = Selector(text=doc_html, type="html")
            filename = a.xpath("//a/text()").get()
            relative_url = a.xpath("//a/@href").get()
            url = urljoin(response.url, relative_url)
            yield Request(
                url=url,
                callback=self.parse_opportunity_document,
                cb_kwargs={
                    "system_id": system_id,
                    "doc_type": "document",
                    "filename": filename,
                    "listing_page": listing_page,
                },
            )

        # And each amendment.
        for doc_html in response.xpath(
            "//table[@id='dataGridBidAmendments']//tr[@class='GridFile1']//td//table//tr[@class='GridFile1']"
        ).getall():
            a = Selector(text=doc_html, type="html")
            date = a.xpath("//td[1]//span[@class='ctext']/text()").get()
            filename = a.xpath("//a/text()").get()
            relative_url = a.xpath("//a/@href").get()
            url = urljoin(response.url, relative_url)
            yield Request(
                url=url,
                callback=self.parse_opportunity_document,
                cb_kwargs={
                    "system_id": system_id,
                    "doc_type": "amendment",
                    "filename": filename,
                    "date": date,
                    "listing_page": listing_page,
                },
            )

        # We need to submit a form to get the page with interested vendors.
        yield FormRequest.from_response(
            response,
            formid="Form1",
            clickdata={"id": "Imagebutton1"},
            callback=self.parse_opportunity_detail_page_vendors,
            cb_kwargs={"first_page": True, "listing_page": listing_page},
        )

        return

    def parse_opportunity_detail_page_vendors(
        self, response: Response, first_page: bool, listing_page: int,
    ):
        self.logger.info("Opportunity detail page with vendors (listing page %d): %s", listing_page, response.url)

        system_id = response.xpath("//span[@id='txtSystemIdentifier']/text()").get()
        item = WebsOpportunityItem()
        item["system_id"] = system_id

        # Each vendor in the <tr> HTML string should have the form:
        # <tr class="GridFile1"><td>company</td><td>email</td><td>phone</td><td>status</td></tr>
        for vendor_tr in response.xpath(
            "//table[@id='Table4']//tr/td[@class='header'][contains(text(), 'Vendors Downloading')]/parent::tr/following-sibling::tr[1]//table[@class='ctext']//tr[@class='GridFile1']"
        ).getall():
            parts = Selector(text=vendor_tr, type="html").xpath("//td/text()").getall()
            if len(parts) != 4:
                self.logger.error(
                    "Failed to parse vendor row: got %d parts, expected 4: %s",
                    len(parts),
                    vendor_tr,
                )
                continue

            # Split status codes into a sorted list.
            status = list(
                sorted([code.strip() for code in parts[3].strip().split("-")])
            )

            vendor = WebsVendorItem()
            vendor["system_id"] = system_id
            vendor["company_name"] = parts[0].strip()
            vendor["email"] = parts[1].strip()
            vendor["phone"] = parts[2].strip()
            vendor["status"] = status
            yield vendor

        if first_page:
            # Submit forms to get the next pages of vendors, but don't have them paginate.
            for page_url in response.xpath(
                "//tr[@class='GridPager']//a/@href"
            ).getall():
                m = DOPOSTBACK_RE.search(page_url)
                if not m:
                    self.logger.error("Failed to parse __doPostBack URL: %s", page_url)
                    continue

                target, argument = m.groups()

                yield FormRequest(
                    url=response.url,
                    formdata={
                        "__EVENTTARGET": target,
                        "__EVENTARGUMENT": argument,
                    },
                    callback=self.parse_opportunity_detail_page_vendors,
                    cb_kwargs={"first_page": False},
                )

        return

    def parse_opportunity_document(
        self,
        response: Response,
        listing_page: int,
        system_id: str,
        doc_type: str,
        filename: str,
        date: Optional[str] = None,
    ):
        self.logger.info("Opportunity document (listing page %d, system_id %s): %s", listing_page, system_id, response.url)
        item = WebsDocumentItem()
        item["system_id"] = system_id
        item["doc_type"] = doc_type
        item["filename"] = filename
        item["date"] = date
        item["original_url"] = response.url
        item["contents"] = response.body
        item["content_type"] = response.headers["content-type"].decode("utf-8")
        yield item
        return
