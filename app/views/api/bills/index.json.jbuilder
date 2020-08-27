@bills.each do |bill|
    json.set! bill.id do
      json.extract! bill, :id, :amount, :due_date, :name, :recurring, :user_id
    end
end