@transactions.each do |transaction|
    json.set! transaction.id do
      json.extract! transaction, :id, :amount, :date, :description, :tags, :transaction_category, :account_id
    end
end