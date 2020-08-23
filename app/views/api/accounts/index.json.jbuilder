@accounts.each do |account|
    json.set! account.id do
      json.extract! account, :id, :label, :account_category, :balance, :debit, :institution, :updated_at, :user_id
    end
end

json.extract! @chart_data