# Define here the models for your scraped items
#
# See documentation in:
# https://docs.scrapy.org/en/latest/topics/items.html

from hashlib import sha256
from scrapy.item import Field, Item
from struct import pack

class WebsOpportunityItem(Item):
    type = Field()
    system_id = Field()
    customer_ref_num = Field()
    org_name = Field()
    title = Field()
    description = Field()
    date_posted = Field()
    date_closed = Field()
    estimated_value = Field()
    contact_name = Field()
    contact_phone = Field()
    contact_email = Field()
    commodity_codes = Field()
    counties = Field()

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self["type"] = "WebsOpportunity"


class WebsVendorItem(Item):
    type = Field()
    system_id = Field()
    company_name = Field()
    email = Field()
    phone = Field()
    status = Field()

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self["type"] = "WebsVendor"


def webs_vendor_sha256(item) -> str:
    hasher = sha256()
    company_name = item["company_name"].encode("utf-8")
    email = item["email"].encode("utf-8")
    phone = item["phone"].encode("utf-8")

    hasher.update(pack("<I", len(company_name)))
    hasher.update(company_name)
    hasher.update(pack("<I", len(email)))
    hasher.update(email)
    hasher.update(pack("<I", len(phone)))
    hasher.update(phone)
    hasher.update(pack("<I", len(item["status"])))
    for el in item["status"]:
        el = el.encode("utf-8")
        hasher.update(pack("<I", len(el)))
        hasher.update(el)
    return hasher.hexdigest()


class WebsDocumentItem(Item):
    type = Field()
    system_id = Field()
    doc_type = Field()
    filename = Field()
    date = Field()
    original_url = Field()
    contents = Field()
    content_type = Field()

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self["type"] = "WebsDocument"
