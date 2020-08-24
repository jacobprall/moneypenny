@goals.each do |goal|
    json.set! goal.id do
      json.extract! goal, :id, :goal_amount, :goal_category, :title, :account_id
    end
end