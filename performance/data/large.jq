.data[] |
select(.status != "deleted") |
{
  date,
  campaign_id,
  campaign_name,
  campaign_status: .status,
  budget,
  currency
} +
(
  .ads[] |
  select(.status != "deleted") |
  {
    ad_id,
    headline,
    ad_status: .status,
    impressions,
    clicks,
    conversions,
    cpc,
    cost
  }
)
