# == Schema Information
#
# Table name: bills
#
#  id         :bigint           not null, primary key
#  amount_due :decimal(8, 2)    not null
#  details    :string
#  due_date   :datetime
#  name       :string           not null
#  paid       :boolean          not null
#  recurring  :integer          not null
#  created_at :datetime         not null
#  updated_at :datetime         not null
#  user_id    :integer
#
# Indexes
#
#  index_bills_on_user_id  (user_id)
#
require 'test_helper'

class BillTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
